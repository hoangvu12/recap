use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::echo::{ECHO_WINDOW_MS, Spoken, is_echo};
use crate::{
    AudioFrame, AudioSource, EngineEvent, LiveEvent, LiveOptions, LiveStt, LiveTranscript,
    MeetingResult, MeetingState, PauseMark, Recorder, Segment, SegmentId, Source,
};

/// How a meeting runs.
#[derive(Clone, Debug, Default)]
pub struct MeetingConfig {
    /// Caption my own speech live too (a second socket). Them is always on.
    pub caption_me: bool,
    pub live: LiveOptions,
    /// Where to record the meeting (Ogg/Opus, ch0 Me, ch1 Them). Paused audio
    /// is left out, so the file's time is meeting time. Complete once
    /// `Finished` is out.
    pub recording: Option<PathBuf>,
}

/// The recorder on a thread of its own: encoding and fsync stay off the
/// runtime.
struct RecorderThread {
    frames: std::sync::mpsc::Sender<AudioFrame>,
    thread: std::thread::JoinHandle<std::io::Result<()>>,
}

impl RecorderThread {
    fn start(path: &Path) -> std::io::Result<RecorderThread> {
        let mut recorder = Recorder::create(path)?;
        let (frames, rx) = std::sync::mpsc::channel::<AudioFrame>();
        let thread = std::thread::Builder::new().name("recap-recorder".into()).spawn(move || {
            for frame in rx {
                recorder.write(&frame)?;
            }
            recorder.finish()
        })?;
        Ok(RecorderThread { frames, thread })
    }

    /// Close the file; any error on the way.
    async fn finish(self) -> std::io::Result<()> {
        drop(self.frames);
        let thread = self.thread;
        tokio::task::spawn_blocking(move || thread.join())
            .await
            .map_err(std::io::Error::other)?
            .map_err(|_| std::io::Error::other("the recorder thread panicked"))?
    }
}

enum Control {
    Pause,
    Resume,
    Stop,
}

/// Pause, resume or stop a running meeting from anywhere (the UI thread, a
/// hotkey). Cheap to clone.
#[derive(Clone)]
pub struct MeetingControl(mpsc::UnboundedSender<Control>);

impl MeetingControl {
    pub fn pause(&self) {
        let _ = self.0.send(Control::Pause);
    }
    pub fn resume(&self) {
        let _ = self.0.send(Control::Resume);
    }
    /// Stop: what is still partial becomes final, then `Stopped` and
    /// `Finished` follow.
    pub fn stop(&self) {
        let _ = self.0.send(Control::Stop);
    }
}

/// A running meeting: its events, and its controls.
pub struct Meeting {
    events: mpsc::UnboundedReceiver<EngineEvent>,
    control: MeetingControl,
}

impl Meeting {
    /// Start recording from `source`, captioning live with `live`. Call it
    /// inside a tokio runtime; the source is read on a thread of its own.
    pub fn start(config: MeetingConfig, source: Box<dyn AudioSource>, live: Arc<dyn LiveStt>) -> Meeting {
        let (event_tx, events) = mpsc::unbounded_channel();
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let (audio_tx, audio_rx) = mpsc::channel(64);
        std::thread::Builder::new()
            .name("recap-audio-source".into())
            .spawn(move || read_source(source, audio_tx))
            .expect("the audio source thread starts");
        tokio::spawn(run(config, live, audio_rx, control_rx, event_tx));
        Meeting { events, control: MeetingControl(control_tx) }
    }

    /// The next event; `None` once `Finished` has been handed out.
    pub async fn next_event(&mut self) -> Option<EngineEvent> {
        self.events.recv().await
    }

    pub fn control(&self) -> MeetingControl {
        self.control.clone()
    }

    pub fn pause(&self) {
        self.control.pause()
    }
    pub fn resume(&self) {
        self.control.resume()
    }
    pub fn stop(&self) {
        self.control.stop()
    }

    /// Its events and its controls apart: the events to whoever shows them,
    /// the controls to whoever drives it.
    pub fn split(self) -> (mpsc::UnboundedReceiver<EngineEvent>, MeetingControl) {
        (self.events, self.control)
    }
}

fn read_source(mut source: Box<dyn AudioSource>, audio: mpsc::Sender<AudioFrame>) {
    while let Some(frame) = source.next_frame() {
        if audio.blocking_send(frame).is_err() {
            return; // the meeting stopped
        }
    }
}

/// One side's live session. How it ended comes on the meeting's `ended`
/// channel, so an error is told at once while the meeting goes on.
struct Side {
    audio: Option<mpsc::UnboundedSender<AudioFrame>>,
    ended: bool,
}

/// Run a side's session; say how it ended.
fn watch_side(
    source: Source,
    task: JoinHandle<Result<(), crate::ProviderError>>,
    ended: mpsc::UnboundedSender<(Source, Result<(), crate::ProviderError>)>,
) {
    tokio::spawn(async move {
        let result = match task.await {
            Ok(result) => result,
            Err(join) => Err(crate::ProviderError::new(crate::ProviderErrorKind::Other, join.to_string())),
        };
        let _ = ended.send((source, result));
    });
}

/// How long after a Me final's end Them's audio may still bring the words it
/// echoes (the window, plus the provider's lag), before the final is kept.
const ECHO_WAIT_MS: u64 = ECHO_WINDOW_MS + 3_000;

/// The engine's own state: what it has heard, and what it tells.
struct Engine {
    events: mpsc::UnboundedSender<EngineEvent>,
    transcript: LiveTranscript,
    finals: Vec<Segment>,
    /// Each side's segment still partial.
    open: HashMap<Source, Segment>,
    next_id: u64,
    /// What Them said, final (to find echoes in).
    them: Vec<Segment>,
    /// How far Them's captions have got, and Them's audio (meeting time).
    them_heard_ms: u64,
    them_audio_ms: u64,
    /// Me finals waiting until Them has caught up, to tell an echo.
    held: Vec<Segment>,
    /// Me segments whose partials are on screen.
    shown: HashSet<SegmentId>,
    pauses: Vec<PauseMark>,
}

impl Engine {
    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }

    /// Whether `me` repeats what Them said (or is saying) about then.
    fn echoes(&self, me: &Segment) -> bool {
        fn spoken(s: &Segment) -> Spoken<'_> {
            Spoken { start_ms: s.start_ms, end_ms: s.end_ms, text: s.text.as_str() }
        }
        let them: Vec<Spoken<'_>> =
            self.them.iter().chain(self.open.get(&Source::Them)).map(spoken).collect();
        is_echo(spoken(me), &them)
    }

    fn show(&mut self, segment: &Segment) {
        self.transcript.apply(segment);
        self.emit(EngineEvent::Segment(segment.clone()));
    }

    /// A final kept for the transcript.
    fn commit(&mut self, segment: Segment) {
        self.finals.push(segment.clone());
        self.emit(EngineEvent::Committed(segment));
    }

    /// Take back Me segment `id` (an echo), if it was shown.
    fn drop_shown(&mut self, id: SegmentId) {
        if self.shown.remove(&id) {
            self.transcript.forget(id);
            self.emit(EngineEvent::Dropped(id));
        }
    }

    fn heard(&mut self, source: Source, event: LiveEvent) {
        let id = match self.open.get(&source) {
            Some(open) => open.id,
            None => {
                self.next_id += 1;
                SegmentId(self.next_id)
            }
        };
        let segment = Segment {
            id,
            source,
            start_ms: event.start_ms,
            end_ms: event.end_ms,
            text: event.text,
            is_final: event.is_final,
        };
        if segment.is_final {
            self.open.remove(&source);
        } else {
            self.open.insert(source, segment.clone());
        }
        match source {
            Source::Them => {
                self.them_heard_ms = self.them_heard_ms.max(segment.end_ms);
                self.show(&segment);
                if segment.is_final {
                    self.them.push(segment.clone());
                    self.commit(segment);
                }
                self.decide(false);
            }
            // Partials are too short to tell an echo by ("One more week"
            // right after Them said it): only finals are judged. A final
            // that already echoes is taken back; else it is shown, and kept
            // once Them has caught up with it.
            Source::Me => {
                if segment.is_final && self.echoes(&segment) {
                    self.drop_shown(segment.id);
                    return;
                }
                self.shown.insert(segment.id);
                self.show(&segment);
                if segment.is_final {
                    self.held.push(segment);
                    self.decide(false);
                }
            }
        }
    }

    /// Them's audio has reached `ms` of meeting time.
    fn them_audio(&mut self, ms: u64) {
        self.them_audio_ms = ms;
        if !self.held.is_empty() {
            self.decide(false);
        }
    }

    /// Keep or drop the held Me finals Them has caught up with (all of them
    /// when `all`: the meeting ended).
    fn decide(&mut self, all: bool) {
        let (them_heard, them_audio) = (self.them_heard_ms, self.them_audio_ms);
        let (ready, held): (Vec<Segment>, Vec<Segment>) = std::mem::take(&mut self.held).into_iter().partition(|me| {
            all || them_heard >= me.end_ms + ECHO_WINDOW_MS || them_audio >= me.end_ms + ECHO_WAIT_MS
        });
        self.held = held;
        for me in ready {
            if self.echoes(&me) {
                self.drop_shown(me.id);
            } else {
                self.shown.remove(&me.id);
                self.commit(me);
            }
        }
    }

    fn mark_pause(&mut self, at_ms: u64, paused_ms: u64) {
        let mark = PauseMark { at_ms, paused_ms, resumed_ms: now_ms().max(paused_ms) };
        self.pauses.push(mark);
        self.transcript.mark_pause(mark);
        self.emit(EngineEvent::Pause(mark));
    }

    /// What is still partial when the meeting ends becomes final.
    fn settle(&mut self) {
        let mut open: Vec<Segment> = self.open.drain().map(|(_, s)| s).collect();
        open.sort_by_key(|s| s.id);
        for segment in open {
            let event = LiveEvent { text: segment.text, is_final: true, start_ms: segment.start_ms, end_ms: segment.end_ms };
            self.heard(segment.source, event);
        }
        self.decide(true);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

async fn run(
    config: MeetingConfig,
    live: Arc<dyn LiveStt>,
    mut audio: mpsc::Receiver<AudioFrame>,
    mut control: mpsc::UnboundedReceiver<Control>,
    events: mpsc::UnboundedSender<EngineEvent>,
) {
    let mut engine = Engine {
        events,
        transcript: LiveTranscript::default(),
        finals: Vec::new(),
        open: HashMap::new(),
        next_id: 0,
        them: Vec::new(),
        them_heard_ms: 0,
        them_audio_ms: 0,
        held: Vec::new(),
        shown: HashSet::new(),
        pauses: Vec::new(),
    };
    engine.emit(EngineEvent::State(MeetingState::Recording));

    let mut recorder = match &config.recording {
        Some(path) => match RecorderThread::start(path) {
            Ok(recorder) => Some(recorder),
            Err(error) => {
                engine.emit(EngineEvent::RecordingFailed(error.to_string()));
                None
            }
        },
        None => None,
    };

    // Them always; Me only when my captions are on.
    let (heard_tx, mut heard) = mpsc::unbounded_channel::<(Source, LiveEvent)>();
    let (ended_tx, mut ended) = mpsc::unbounded_channel::<(Source, Result<(), crate::ProviderError>)>();
    let mut sides: HashMap<Source, Side> = HashMap::new();
    let wanted = if config.caption_me { vec![Source::Them, Source::Me] } else { vec![Source::Them] };
    for source in wanted {
        let (audio_tx, audio_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let task = live.run(source, config.live.clone(), audio_rx, event_tx);
        let forward = heard_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                let _ = forward.send((source, event));
            }
        });
        watch_side(source, tokio::spawn(task), ended_tx.clone());
        sides.insert(source, Side { audio: Some(audio_tx), ended: false });
    }
    drop(heard_tx);
    drop(ended_tx);

    // While paused: when it paused (wall clock).
    let mut paused: Option<u64> = None;
    let mut elapsed_ms = 0;
    let mut next_tick = 1_000;
    loop {
        tokio::select! {
            biased;
            Some(command) = control.recv() => match command {
                Control::Pause if paused.is_none() => {
                    paused = Some(now_ms());
                    engine.emit(EngineEvent::State(MeetingState::Paused));
                }
                Control::Resume if paused.is_some() => {
                    if let Some(at) = paused.take() {
                        engine.mark_pause(elapsed_ms, at);
                    }
                    engine.emit(EngineEvent::State(MeetingState::Recording));
                }
                Control::Stop => break,
                _ => {}
            },
            Some((source, event)) = heard.recv() => engine.heard(source, event),
            // A side's captions ended early (its provider and backup gave
            // up): tell it now; the recording goes on.
            Some((source, result)) = ended.recv() => {
                if let Some(side) = sides.get_mut(&source) {
                    side.ended = true;
                    side.audio = None;
                }
                if let Err(error) = result {
                    engine.emit(EngineEvent::Error { source, error });
                }
            }
            frame = audio.recv() => {
                let Some(frame) = frame else { break };
                if paused.is_some() {
                    continue;
                }
                if let Some(rec) = &recorder
                    && rec.frames.send(frame.clone()).is_err()
                {
                    // The recorder stopped on an error; finish() reports it.
                    if let Some(rec) = recorder.take()
                        && let Err(error) = rec.finish().await
                    {
                        engine.emit(EngineEvent::RecordingFailed(error.to_string()));
                    }
                }
                if frame.source == Source::Them {
                    elapsed_ms += frame.duration_ms();
                    while elapsed_ms >= next_tick {
                        engine.emit(EngineEvent::Elapsed(next_tick));
                        next_tick += 1_000;
                    }
                    engine.them_audio(elapsed_ms);
                }
                if let Some(side) = sides.get(&frame.source)
                    && let Some(tx) = &side.audio
                {
                    let _ = tx.send(frame);
                }
            }
        }
    }
    // Stopped while paused: the pause still gets its marker.
    if let Some(at) = paused.take() {
        engine.mark_pause(elapsed_ms, at);
    }
    drop(audio); // the source thread ends at its next frame
    if let Some(rec) = recorder.take()
        && let Err(error) = rec.finish().await
    {
        engine.emit(EngineEvent::RecordingFailed(error.to_string()));
    }

    // Close each side's audio; the sessions say the rest of what they heard.
    for side in sides.values_mut() {
        side.audio.take();
    }
    let mut running = sides.values().filter(|side| !side.ended).count();
    while running > 0 {
        let Some((source, result)) = ended.recv().await else { break };
        running -= 1;
        if let Err(error) = result {
            engine.emit(EngineEvent::Error { source, error });
        }
    }
    while let Some((source, event)) = heard.recv().await {
        engine.heard(source, event);
    }
    engine.settle();
    engine.transcript.finish();

    engine.emit(EngineEvent::State(MeetingState::Stopped));
    let result = MeetingResult {
        pauses: std::mem::take(&mut engine.pauses),
        segments: std::mem::take(&mut engine.finals),
        rows: engine.transcript.rows().to_vec(),
        duration_ms: elapsed_ms,
    };
    engine.emit(EngineEvent::Finished(result));
}
