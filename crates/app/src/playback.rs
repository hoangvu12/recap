//! Playback of a stored meeting's recording (`audio.ogg`): a line of the
//! transcript from where it starts (both sides, to the end), or a voice's
//! clip in the Name speakers dialog (its longest turns, its side only).
//!
//! [`Player`] plays on a thread of its own: it decodes the recording
//! ([`Recording::read`], the last one kept), cuts the [`Clip`] and hands it
//! to an [`Output`] (rodio's default device in the app, [`RodioOutput`]; a
//! fake in tests). One clip at a time: a new one ends the one playing. It
//! tells how each goes as [`PlayerEvent`]s.

use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self as std_mpsc, RecvTimeoutError},
    time::Duration,
};

use recap_engine::{Recording, SAMPLE_RATE, Source};
use recap_storage::Segment;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// A voice's clip is at most this long.
pub const CLIP_MS: u64 = 8_000;
/// Silence between the turns of a clip.
pub const GAP_MS: u64 = 300;

/// Audio to play: 16 kHz mono.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clip {
    pub samples: Vec<i16>,
}

impl Clip {
    pub fn duration_ms(&self) -> u64 {
        self.samples.len() as u64 * 1000 / u64::from(SAMPLE_RATE)
    }
}

fn sample(ms: u64) -> usize {
    (ms * u64::from(SAMPLE_RATE) / 1000) as usize
}

/// From `from_ms` (meeting time) to the end, Me and Them together.
pub fn from_line(rec: &Recording, from_ms: u64) -> Clip {
    let len = rec.me.len().max(rec.them.len());
    let start = sample(from_ms).min(len);
    let at = |side: &[i16], i: usize| side.get(i).copied().unwrap_or(0);
    Clip { samples: (start..len).map(|i| at(&rec.me, i).saturating_add(at(&rec.them, i))).collect() }
}

/// The turns a voice's clip plays: its longest ones, up to [`CLIP_MS`] in
/// all (the last cut to fit), in the order said.
pub fn speaker_turns(segments: &[Segment], speaker: &str) -> Vec<(u64, u64)> {
    let mut turns: Vec<(u64, u64)> = segments
        .iter()
        .filter(|s| s.speaker.as_deref() == Some(speaker) && s.end_ms > s.start_ms)
        .map(|s| (s.start_ms, s.end_ms))
        .collect();
    turns.sort_by_key(|&(start, end)| (std::cmp::Reverse(end - start), start));
    let mut out = Vec::new();
    let mut total = 0;
    for (start, end) in turns {
        if total >= CLIP_MS {
            break;
        }
        let end = end.min(start + CLIP_MS - total);
        total += end - start;
        out.push((start, end));
    }
    out.sort();
    out
}

/// Which side a speaker is heard on: Me on the mic, the voices on the
/// meeting audio.
pub fn side_of(speaker: &str) -> Source {
    if speaker == "me" { Source::Me } else { Source::Them }
}

/// `turns` of one side, a short silence between them.
pub fn cut(rec: &Recording, source: Source, turns: &[(u64, u64)]) -> Clip {
    let side = match source {
        Source::Me => &rec.me,
        Source::Them => &rec.them,
    };
    let mut samples = Vec::new();
    for (i, &(start, end)) in turns.iter().enumerate() {
        if i > 0 {
            samples.extend(std::iter::repeat_n(0, sample(GAP_MS)));
        }
        let (a, b) = (sample(start).min(side.len()), sample(end).min(side.len()));
        samples.extend_from_slice(&side[a..b]);
    }
    Clip { samples }
}

/// Speaker `speaker`'s clip from the refined transcript's `segments`.
pub fn speaker_clip(rec: &Recording, segments: &[Segment], speaker: &str) -> Clip {
    cut(rec, side_of(speaker), &speaker_turns(segments, speaker))
}

/// What to play of a recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    /// A line: from its start to the end.
    Line { from_ms: u64 },
    /// A voice: these turns of its side.
    Speaker { source: Source, turns: Vec<(u64, u64)> },
}

/// Where sound goes. Made on the player's thread, used only there.
pub trait Output {
    fn play(&mut self, clip: Clip) -> Result<(), String>;
    /// Still playing what it was given.
    fn playing(&self) -> bool;
    fn stop(&mut self);
}

/// How playback goes, by the key it was asked with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerEvent {
    Playing { key: String, duration_ms: u64 },
    /// Done, stopped, or ended by another.
    Ended { key: String },
    Failed { key: String, error: String },
}

enum Command {
    Play { key: String, audio: PathBuf, what: What },
    Stop,
}

/// The playback thread. It ends when this is dropped.
pub struct Player {
    commands: std_mpsc::Sender<Command>,
    /// The thread didn't start: every play fails, told with this.
    broken: Option<(String, UnboundedSender<PlayerEvent>)>,
}

impl Player {
    /// Start the thread; `output` makes its output there (on the first clip,
    /// so a machine with no sound device only fails when asked to play).
    /// A thread that can't start fails each play the same way.
    pub fn spawn(output: impl FnOnce() -> Box<dyn Output> + Send + 'static) -> (Player, UnboundedReceiver<PlayerEvent>) {
        let (commands, inbox) = std_mpsc::channel();
        let (events, out) = mpsc::unbounded_channel();
        let started = std::thread::Builder::new().name("recap-playback".into()).spawn({
            let events = events.clone();
            move || run(inbox, events, output)
        });
        let broken = started.err().map(|error| (format!("Playback can't start: {error}"), events));
        (Player { commands, broken }, out)
    }

    /// Play `what` of the recording at `audio`, told as `key`.
    pub fn play(&self, key: &str, audio: PathBuf, what: What) {
        if let Some((error, events)) = &self.broken {
            let _ = events.send(PlayerEvent::Failed { key: key.to_owned(), error: error.clone() });
            return;
        }
        let _ = self.commands.send(Command::Play { key: key.to_owned(), audio, what });
    }

    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }
}

fn run(inbox: std_mpsc::Receiver<Command>, events: UnboundedSender<PlayerEvent>, make: impl FnOnce() -> Box<dyn Output>) {
    let mut make = Some(make);
    let mut output: Option<Box<dyn Output>> = None;
    let mut current: Option<String> = None;
    let mut decoded: Option<(PathBuf, Recording)> = None;
    let tell = |event: PlayerEvent| drop(events.send(event));
    let stop_current = |output: &mut Option<Box<dyn Output>>, current: &mut Option<String>| {
        if let Some(key) = current.take() {
            if let Some(output) = output.as_mut() {
                output.stop();
            }
            tell(PlayerEvent::Ended { key });
        }
    };
    loop {
        let command = match inbox.recv_timeout(Duration::from_millis(50)) {
            Ok(command) => command,
            Err(RecvTimeoutError::Timeout) => {
                if current.is_some() && !output.as_ref().is_some_and(|o| o.playing()) {
                    tell(PlayerEvent::Ended { key: current.take().unwrap_or_default() });
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let (key, audio, what) = match command {
            Command::Stop => {
                stop_current(&mut output, &mut current);
                continue;
            }
            Command::Play { key, audio, what } => (key, audio, what),
        };
        stop_current(&mut output, &mut current);
        if decoded.as_ref().is_none_or(|(path, _)| *path != audio) {
            decoded = None;
            match read(&audio) {
                Ok(rec) => decoded = Some((audio, rec)),
                Err(error) => {
                    tell(PlayerEvent::Failed { key, error });
                    continue;
                }
            }
        }
        let rec = &decoded.as_ref().expect("decoded above").1;
        let clip = match &what {
            What::Line { from_ms } => from_line(rec, *from_ms),
            What::Speaker { source, turns } => cut(rec, *source, turns),
        };
        if clip.samples.is_empty() {
            tell(PlayerEvent::Failed { key, error: "Nothing was recorded there.".into() });
            continue;
        }
        let duration_ms = clip.duration_ms();
        let output = match (&mut output, make.take()) {
            (Some(output), _) => output,
            (slot, Some(make)) => slot.insert(make()),
            (None, None) => unreachable!("an output is made once, then kept"),
        };
        match output.play(clip) {
            Ok(()) => {
                current = Some(key.clone());
                tell(PlayerEvent::Playing { key, duration_ms });
            }
            Err(error) => tell(PlayerEvent::Failed { key, error }),
        }
    }
    if let Some(output) = output.as_mut() {
        output.stop();
    }
}

fn read(audio: &Path) -> Result<Recording, String> {
    if !audio.exists() {
        return Err("This meeting's recording is gone.".into());
    }
    Recording::read(audio).map_err(|e| format!("The recording can't be read: {e}"))
}

/// The default output device, through rodio, opened at the first clip.
#[derive(Default)]
pub struct RodioOutput {
    sink: Option<rodio::MixerDeviceSink>,
    player: Option<rodio::Player>,
}

impl Output for RodioOutput {
    fn play(&mut self, clip: Clip) -> Result<(), String> {
        if self.sink.is_none() {
            let mut sink = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| format!("No sound output: {e}"))?;
            sink.log_on_drop(false);
            self.sink = Some(sink);
        }
        let sink = self.sink.as_ref().expect("opened above");
        if let Some(player) = self.player.take() {
            player.stop();
        }
        let player = rodio::Player::connect_new(sink.mixer());
        let samples: Vec<f32> = clip.samples.iter().map(|&s| f32::from(s) / 32_768.0).collect();
        let channels = std::num::NonZero::new(1).expect("one channel");
        let rate = std::num::NonZero::new(SAMPLE_RATE).expect("a sample rate");
        player.append(rodio::buffer::SamplesBuffer::new(channels, rate, samples));
        self.player = Some(player);
        Ok(())
    }

    fn playing(&self) -> bool {
        self.player.as_ref().is_some_and(|p| !p.empty())
    }

    fn stop(&mut self) {
        if let Some(player) = self.player.take() {
            player.stop();
        }
    }
}
