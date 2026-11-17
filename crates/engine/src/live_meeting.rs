use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::echo::{ECHO_WINDOW_MS, Spoken, is_echo};
use crate::{
    AudioFrame, AudioSource, EngineEvent, LiveEvent, LiveOptions, LiveStatus, LiveStt, LiveTranscript,
    MeetingResult, MeetingState, PauseMark, Recorder, Segment, SegmentId, Source,
};

/// How a meeting runs.
#[derive(Clone, Debug, Default)]
pub struct MeetingConfig {
    /// Both sides are always transcribed live: Me feeds the Notes transcript
    /// and the echo check. Whether my lines show in the overlay is the app's
    /// choice (ADR 0019).
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
    /// Caption the sides whose provider failed with this one.
    Live(Arc<dyn LiveStt>),
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
    /// Caption with `live` from now on, on the sides whose provider failed
    /// ("Switch to Deepgram"); the others keep theirs.
    pub fn replace_live(&self, live: Arc<dyn LiveStt>) {
        let _ = self.0.send(Control::Live(live));
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

/// One side's live session.
struct Side {
    audio: Option<mpsc::UnboundedSender<AudioFrame>>,
    task: JoinHandle<Result<(), crate::ProviderError>>,
    /// Which start of the side this is (a side may be started again).
    generation: u64,
}

/// What a side's session tells the engine.
enum Heard {
    Event(LiveEvent),
    Status(LiveStatus),
}

/// The sides being captioned live, and how to start one.
struct Sides {
    live: Arc<dyn LiveStt>,
    options: LiveOptions,
    /// The sides that should be captioned (both, always).
    wanted: Vec<Source>,
    running: HashMap<Source, Side>,
    heard: mpsc::UnboundedSender<(Source, Heard)>,
    /// A side's session ended (by its generation).
    ended: mpsc::UnboundedSender<(Source, u64)>,
    generations: u64,
}

impl Sides {
    /// Caption `source` with the current provider, from now on.
    fn start(&mut self, source: Source, engine: &Engine) {
        let (audio_tx, audio_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
        let session = self.live.run_reporting(source, self.options.clone(), audio_rx, event_tx, status_tx);
        engine.emit(EngineEvent::Live { source, status: LiveStatus::Using(self.live.name().to_owned()) });
        let forward = self.heard.clone();
        // A status said before an event is forwarded before it.
        tokio::spawn(async move {
            let mut status_open = true;
            loop {
                tokio::select! {
                    biased;
                    status = status_rx.recv(), if status_open => match status {
                        Some(status) => drop(forward.send((source, Heard::Status(status)))),
                        None => status_open = false,
                    },
                    event = event_rx.recv() => match event {
                        Some(event) => drop(forward.send((source, Heard::Event(event)))),
                        None => break,
                    },
                }
            }
        });
        self.generations += 1;
        let (generation, ended) = (self.generations, self.ended.clone());
        let task = tokio::spawn(async move {
            let result = session.await;
            let _ = ended.send((source, generation));
            result
        });
        self.running.insert(source, Side { audio: Some(audio_tx), task, generation });
    }

    /// Start the wanted sides that aren't running.
    fn start_missing(&mut self, engine: &Engine) {
        for source in self.wanted.clone() {
            if !self.running.contains_key(&source) {
                self.start(source, engine);
            }
        }
    }
}

/// Tell what a side's session ended with, if it failed.
fn report(engine: &Engine, source: Source, ended: Result<Result<(), crate::ProviderError>, tokio::task::JoinError>) {
    let error = match ended {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(join) => crate::ProviderError::new(crate::ProviderErrorKind::Other, join.to_string()),
    };
    engine.emit(EngineEvent::Error { source, error });
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

    // Both sides, always: Me feeds the Notes transcript and the echo check.
    let (heard_tx, mut heard) = mpsc::unbounded_channel::<(Source, Heard)>();
    let (ended_tx, mut ended) = mpsc::unbounded_channel::<(Source, u64)>();
    let mut sides = Sides {
        live,
        options: config.live.clone(),
        wanted: vec![Source::Them, Source::Me],
        running: HashMap::new(),
        heard: heard_tx,
        ended: ended_tx,
        generations: 0,
    };
    sides.start_missing(&engine);

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
                Control::Live(live) => {
                    sides.live = live;
                    sides.start_missing(&engine);
                }
                _ => {}
            },
            Some((source, heard)) = heard.recv() => match heard {
                Heard::Event(event) => engine.heard(source, event),
                Heard::Status(status) => engine.emit(EngineEvent::Live { source, status }),
            },
            Some((source, generation)) = ended.recv() => {
                // A session ended mid-meeting (it failed): told at once.
                if sides.running.get(&source).is_some_and(|side| side.generation == generation)
                    && let Some(side) = sides.running.remove(&source)
                {
                    report(&engine, source, side.task.await);
                }
            },
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
                if let Some(side) = sides.running.get(&frame.source)
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
    let mut running = std::mem::take(&mut sides.running);
    drop(sides);
    for side in running.values_mut() {
        side.audio.take();
    }
    let mut order: Vec<Source> = running.keys().copied().collect();
    order.sort();
    for source in order {
        let side = running.remove(&source).expect("each side once");
        report(&engine, source, side.task.await);
    }
    while let Some((source, heard)) = heard.recv().await {
        if let Heard::Event(event) = heard {
            engine.heard(source, event);
        }
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
