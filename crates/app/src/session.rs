//! A live meeting, saved as it goes (GLOSSARY: Live session): the engine's
//! meeting plus the store. No zgx: the host shows its events.
//!
//! - **Start:** the meeting is created in the store (status recording) and
//!   recorded to `audio.ogg` in its folder.
//! - **While it runs:** every final the engine keeps (`Committed`: Them at
//!   once, Me once it is no echo) is appended to `live.ndjson` and the index.
//! - **Stop:** its duration and status (processing: ready for the after-Stop
//!   pipeline) are saved, and its pauses in `pauses.json`; then `Saved`.
//!
//! Without a store (it could not open) the meeting runs unsaved.

use std::sync::Arc;

use recap_engine::{
    AudioSource, EngineEvent, LiveOptions, LiveStt, Meeting, MeetingConfig, MeetingControl, MeetingResult,
};
use recap_storage::{MeetingId, MeetingInfo, Status, Store};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Where a meeting's pauses are kept, in its folder.
pub const PAUSES: &str = "pauses.json";

#[derive(Clone, Debug, Default)]
pub struct SessionConfig {
    /// When Start was pressed, ms since the epoch.
    pub started_ms: u64,
    /// "Caption my own speech": a second live socket for Me.
    pub caption_me: bool,
    pub live: LiveOptions,
}

/// What a live session tells the UI.
#[derive(Clone, Debug)]
pub enum SessionEvent {
    /// Everything the engine says, `Finished` last of these.
    Engine(EngineEvent),
    /// Something the user should know (the backup took over, a save failed).
    Notice(String),
    /// The meeting is saved, stopped, ready for processing. The last event.
    Saved(MeetingInfo),
}

pub struct LiveSession {
    /// The stored meeting; none when it runs unsaved.
    pub id: Option<MeetingId>,
    pub events: UnboundedReceiver<SessionEvent>,
    pub control: MeetingControl,
}

/// A channel for notices: give the sender to whoever has news (the live
/// job's `on_switch`), the receiver to [`start`].
pub fn notices() -> (impl Fn(String) + Clone + Send + Sync + 'static, UnboundedReceiver<String>) {
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    (move |text: String| drop(tx.send(text)), rx)
}

/// Start a meeting from `source`, captioned by `live`, saved in `store`. Call
/// it inside the engine's tokio runtime.
pub fn start(
    store: Option<Store>,
    config: SessionConfig,
    source: Box<dyn AudioSource>,
    live: Arc<dyn LiveStt>,
    notices: Option<UnboundedReceiver<String>>,
) -> LiveSession {
    let (out, events) = mpsc::unbounded_channel();
    let saved = store.and_then(|store| match store.create(config.started_ms) {
        Ok(info) => Some((store, info.id)),
        Err(error) => {
            let _ = out.send(SessionEvent::Notice(format!("This meeting can't be saved: {error}")));
            None
        }
    });
    let recording = saved.as_ref().map(|(store, id)| store.folder(id).join("audio.ogg"));
    let meeting = Meeting::start(
        MeetingConfig { caption_me: config.caption_me, live: config.live, recording },
        source,
        live,
    );
    let (engine, control) = meeting.split();
    let id = saved.as_ref().map(|(_, id)| id.clone());
    tokio::spawn(run(engine, notices, saved, out));
    LiveSession { id, events, control }
}

async fn run(
    mut engine: UnboundedReceiver<EngineEvent>,
    mut notices: Option<UnboundedReceiver<String>>,
    saved: Option<(Store, MeetingId)>,
    out: UnboundedSender<SessionEvent>,
) {
    let send = |event: SessionEvent| drop(out.send(event));
    let mut result: Option<MeetingResult> = None;
    loop {
        let notice = async {
            match notices.as_mut() {
                Some(rx) => rx.recv().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            biased;
            event = engine.recv() => {
                let Some(event) = event else { break };
                if let (EngineEvent::Committed(segment), Some((store, id))) = (&event, &saved)
                    && let Err(error) = store.append_live(id, &[stored(segment)])
                {
                    send(SessionEvent::Notice(format!("The transcript can't be saved: {error}")));
                }
                if let EngineEvent::Finished(r) = &event {
                    result = Some(r.clone());
                }
                send(SessionEvent::Engine(event));
            }
            Some(text) = notice => send(SessionEvent::Notice(text)),
            else => break,
        }
    }
    let (Some((store, id)), Some(result)) = (saved, result) else { return };
    if !result.pauses.is_empty() {
        let path = store.folder(&id).join(PAUSES);
        let written = serde_json::to_vec_pretty(&result.pauses).map_err(std::io::Error::other).and_then(|json| std::fs::write(&path, json));
        if let Err(error) = written {
            send(SessionEvent::Notice(format!("{}: {error}", path.display())));
        }
    }
    match store.update(&id, |info| {
        info.duration_ms = result.duration_ms;
        info.status = Status::Processing;
    }) {
        Ok(info) => send(SessionEvent::Saved(info)),
        Err(error) => send(SessionEvent::Notice(format!("This meeting can't be saved: {error}"))),
    }
}

fn stored(segment: &recap_engine::Segment) -> recap_storage::Segment {
    recap_storage::Segment {
        source: segment.source,
        speaker: None,
        start_ms: segment.start_ms,
        end_ms: segment.end_ms,
        text: segment.text.clone(),
    }
}
