use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::{AudioFrame, AudioSource, Packet, Source, Timeline};

/// An [`AudioSource`] that reads WAV files instead of devices: tests and
/// replays. It goes through the same [`Timeline`] as live capture, so it
/// hands out what live capture does: 16 kHz mono frames, Me and Them aligned,
/// a side with nothing to say zero-filled.
pub struct WavSource {
    inputs: Vec<Input>,
    timeline: Timeline,
    queue: VecDeque<AudioFrame>,
    finished: bool,
    paced: Option<Instant>,
}

/// How long a side may lag before it is zero-filled: longer than the
/// resampler holds audio back.
const WAIT: Duration = Duration::from_millis(300);
const STEP_MS: u64 = 100;

impl WavSource {
    /// One file. Stereo (a recording's layout) is Me on channel 0 and Them on
    /// channel 1; mono is Them alone, Me silent.
    pub fn open(path: &Path) -> io::Result<WavSource> {
        let input = Input::open(path, Duration::ZERO, None)?;
        Ok(Self::from_inputs(vec![input]))
    }

    /// A file per side, each with where on the meeting's timeline it starts.
    /// All of a file's channels are mixed into its side. A missing side is
    /// silent.
    pub fn sides(me: Option<(&Path, Duration)>, them: Option<(&Path, Duration)>) -> io::Result<WavSource> {
        let mut inputs = Vec::new();
        for (side, source) in [(me, Source::Me), (them, Source::Them)] {
            if let Some((path, starts_at)) = side {
                inputs.push(Input::open(path, starts_at, Some(source))?);
            }
        }
        Ok(Self::from_inputs(inputs))
    }

    /// Hand frames out in real time, as live capture would (the app on a
    /// replay); otherwise as fast as they are read.
    pub fn paced(mut self) -> Self {
        self.paced = Some(Instant::now());
        self
    }

    fn from_inputs(inputs: Vec<Input>) -> WavSource {
        WavSource { inputs, timeline: Timeline::new(), queue: VecDeque::new(), finished: false, paced: None }
    }

    /// Read the next step of every file into the timeline.
    fn step(&mut self) {
        for input in &mut self.inputs {
            for (source, packet) in input.read(STEP_MS) {
                self.timeline.push(source, packet);
            }
        }
        let now = self.inputs.iter().filter(|i| !i.done).map(Input::position).min();
        match now {
            Some(now) => self.queue.extend(self.timeline.frames(now, WAIT)),
            None => {
                self.queue.extend(self.timeline.finish());
                self.finished = true;
            }
        }
    }
}

impl AudioSource for WavSource {
    fn next_frame(&mut self) -> Option<AudioFrame> {
        while self.queue.is_empty() && !self.finished {
            self.step();
        }
        let frame = self.queue.pop_front()?;
        if let Some(started) = self.paced
            && frame.source == Source::Them
        {
            let due = started + Duration::from_millis(frame.start_ms + frame.duration_ms());
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
        Some(frame)
    }
}

/// One WAV file played again and again, its rounds one after another on the
/// meeting's timeline: the soak run's endless meeting. Ends only when the
/// meeting stops (or the file can't be read again).
pub struct LoopingWav {
    path: std::path::PathBuf,
    round: WavSource,
    /// Where this round starts on the meeting's timeline.
    offset_ms: u64,
    /// The end of the furthest frame of this round, on the round's timeline.
    round_end_ms: u64,
    paced: Option<Instant>,
}

impl LoopingWav {
    pub fn open(path: &Path) -> io::Result<LoopingWav> {
        Ok(LoopingWav { path: path.to_owned(), round: WavSource::open(path)?, offset_ms: 0, round_end_ms: 0, paced: None })
    }

    /// In real time, as live capture would be.
    pub fn paced(mut self) -> Self {
        self.paced = Some(Instant::now());
        self
    }
}

impl AudioSource for LoopingWav {
    fn next_frame(&mut self) -> Option<AudioFrame> {
        let mut frame = match self.round.next_frame() {
            Some(frame) => frame,
            None => {
                // The round has ended: the next starts where it stopped.
                self.round = WavSource::open(&self.path).ok()?;
                self.offset_ms += self.round_end_ms;
                self.round_end_ms = 0;
                self.round.next_frame()?
            }
        };
        self.round_end_ms = self.round_end_ms.max(frame.start_ms + frame.duration_ms());
        frame.start_ms += self.offset_ms;
        if let Some(started) = self.paced
            && frame.source == Source::Them
        {
            let due = started + Duration::from_millis(frame.start_ms + frame.duration_ms());
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
        Some(frame)
    }
}

/// One WAV file being read.
struct Input {
    reader: hound::WavReader<BufReader<File>>,
    /// `None`: channel 0 is Me, channel 1 (if any) Them; mono is Them.
    side: Option<Source>,
    rate: u32,
    channels: u16,
    scale: Option<f32>,
    starts_at: Duration,
    frames_read: u64,
    done: bool,
}

impl Input {
    fn open(path: &Path, starts_at: Duration, side: Option<Source>) -> io::Result<Input> {
        let reader = hound::WavReader::open(path).map_err(io::Error::other)?;
        let spec = reader.spec();
        let scale = match spec.sample_format {
            hound::SampleFormat::Int => Some(1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32),
            hound::SampleFormat::Float => None,
        };
        Ok(Input {
            reader,
            side,
            rate: spec.sample_rate,
            channels: spec.channels,
            scale,
            starts_at,
            frames_read: 0,
            done: false,
        })
    }

    /// Where on the meeting's timeline this file has been read up to.
    fn position(&self) -> Duration {
        self.starts_at + Duration::from_secs_f64(self.frames_read as f64 / self.rate as f64)
    }

    fn read(&mut self, ms: u64) -> Vec<(Source, Packet)> {
        if self.done {
            return Vec::new();
        }
        let at = self.position();
        let want = (self.rate as u64 * ms / 1000) as usize * self.channels as usize;
        let samples: Vec<f32> = match self.scale {
            Some(scale) => {
                self.reader.samples::<i32>().take(want).filter_map(Result::ok).map(|s| s as f32 * scale).collect()
            }
            None => self.reader.samples::<f32>().take(want).filter_map(Result::ok).collect(),
        };
        let channels = self.channels as usize;
        let frames = samples.len() / channels;
        if frames < want / channels {
            self.done = true;
        }
        if frames == 0 {
            return Vec::new();
        }
        self.frames_read += frames as u64;
        let packet = |source, channels: u16, samples| (source, Packet { at, rate: self.rate, channels, samples });
        match (self.side, channels) {
            (Some(source), _) => vec![packet(source, self.channels, samples)],
            (None, 1) => vec![packet(Source::Them, 1, samples)],
            (None, _) => {
                let channel = |c: usize| samples.chunks_exact(channels).map(|f| f[c]).collect::<Vec<f32>>();
                vec![packet(Source::Me, 1, channel(0)), packet(Source::Them, 1, channel(1))]
            }
        }
    }
}
