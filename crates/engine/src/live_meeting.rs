use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{
    AudioFrame, AudioSource, EngineEvent, LiveEvent, LiveOptions, LiveStt, LiveTranscript,
    MeetingResult, MeetingState, Segment, SegmentId, Source,
};

/// How a meeting runs.
#[derive(Clone, Debug, Default)]
pub struct MeetingConfig {
    /// Caption my own speech live too (a second socket). Them is always on.
    pub caption_me: bool,
    pub live: LiveOptions,
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

/// One side's live session.
struct Side {
    audio: Option<mpsc::UnboundedSender<AudioFrame>>,
    task: JoinHandle<Result<(), crate::ProviderError>>,
}

/// The engine's own state: what it has heard, and what it tells.
struct Engine {
    events: mpsc::UnboundedSender<EngineEvent>,
    transcript: LiveTranscript,
    finals: Vec<Segment>,
    /// Each side's segment still partial.
    open: HashMap<Source, Segment>,
    next_id: u64,
}

impl Engine {
    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
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
            self.finals.push(segment.clone());
        } else {
            self.open.insert(source, segment.clone());
        }
        self.transcript.apply(&segment);
        self.emit(EngineEvent::Segment(segment));
    }

    /// What is still partial when the meeting ends becomes final.
    fn settle(&mut self) {
        let mut open: Vec<Segment> = self.open.drain().map(|(_, s)| s).collect();
        open.sort_by_key(|s| s.id);
        for segment in open {
            let event = LiveEvent { text: segment.text, is_final: true, start_ms: segment.start_ms, end_ms: segment.end_ms };
            self.heard(segment.source, event);
        }
    }
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
    };
    engine.emit(EngineEvent::State(MeetingState::Recording));

    // Them always; Me only when my captions are on.
    let (heard_tx, mut heard) = mpsc::unbounded_channel::<(Source, LiveEvent)>();
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
        sides.insert(source, Side { audio: Some(audio_tx), task: tokio::spawn(task) });
    }
    drop(heard_tx);

    let mut paused = false;
    let mut elapsed_ms = 0;
    let mut next_tick = 1_000;
    loop {
        tokio::select! {
            biased;
            Some(command) = control.recv() => match command {
                Control::Pause if !paused => {
                    paused = true;
                    engine.emit(EngineEvent::State(MeetingState::Paused));
                }
                Control::Resume if paused => {
                    paused = false;
                    engine.emit(EngineEvent::State(MeetingState::Recording));
                }
                Control::Stop => break,
                _ => {}
            },
            Some((source, event)) = heard.recv() => engine.heard(source, event),
            frame = audio.recv() => {
                let Some(frame) = frame else { break };
                if paused {
                    continue;
                }
                if frame.source == Source::Them {
                    elapsed_ms += frame.duration_ms();
                    while elapsed_ms >= next_tick {
                        engine.emit(EngineEvent::Elapsed(next_tick));
                        next_tick += 1_000;
                    }
                }
                if let Some(side) = sides.get(&frame.source) {
                    if let Some(tx) = &side.audio {
                        let _ = tx.send(frame);
                    }
                }
            }
        }
    }
    drop(audio); // the source thread ends at its next frame

    // Close each side's audio; the sessions say the rest of what they heard.
    for side in sides.values_mut() {
        side.audio.take();
    }
    let mut order: Vec<Source> = sides.keys().copied().collect();
    order.sort();
    for source in order {
        let side = sides.remove(&source).expect("each side once");
        match side.task.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => engine.emit(EngineEvent::Error { source, error }),
            Err(join) => engine.emit(EngineEvent::Error {
                source,
                error: crate::ProviderError::new(crate::ProviderErrorKind::Other, join.to_string()),
            }),
        }
    }
    while let Some((source, event)) = heard.recv().await {
        engine.heard(source, event);
    }
    engine.settle();
    engine.transcript.finish();

    engine.emit(EngineEvent::State(MeetingState::Stopped));
    let result = MeetingResult {
        segments: std::mem::take(&mut engine.finals),
        rows: engine.transcript.rows().to_vec(),
        duration_ms: elapsed_ms,
    };
    engine.emit(EngineEvent::Finished(result));
}
