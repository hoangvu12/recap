//! Seam 1 for a live meeting: the WAV source and a fake live provider
//! through the real engine and the real store. What the user sees (events)
//! and what is left on disk (the meeting's folder and its index).

use std::{sync::Arc, time::Duration};

use recap::session::{self, SessionConfig, SessionEvent};
use recap_engine::{
    EngineEvent, FrameFeed, MeetingState, PauseMark, Script, ScriptedLiveStt, Segment, Source, WavSource,
};
use recap_storage::{Status, Store, TranscriptKind};

const STARTED: u64 = 1_794_830_400_000;
const WAV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/meeting_tts.wav");

fn config(caption_me: bool) -> SessionConfig {
    SessionConfig { started_ms: STARTED, caption_me, live: Default::default() }
}

fn scripted() -> Arc<ScriptedLiveStt> {
    Arc::new(ScriptedLiveStt::new(Script::test_meeting()))
}

/// Every event until the session ends.
async fn drain(events: &mut tokio::sync::mpsc::UnboundedReceiver<SessionEvent>) -> Vec<SessionEvent> {
    let mut out = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_secs(30), events.recv()).await.expect("the session keeps talking") {
            Some(event) => out.push(event),
            None => return out,
        }
    }
}

fn committed(events: &[SessionEvent]) -> Vec<Segment> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Engine(EngineEvent::Committed(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_replayed_meeting_is_saved_as_it_goes_and_left_for_processing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let source = WavSource::open(std::path::Path::new(WAV)).unwrap();
    let mut live = session::start(Some(store.clone()), config(true), Box::new(source), scripted(), None);

    // Created at Start: listed while it records.
    let id = live.id.clone().expect("a stored meeting");
    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].id.clone(), listed[0].status, listed[0].started_ms), (id.clone(), Status::Recording, STARTED));

    let events = drain(&mut live.events).await;

    // The UI saw the meeting live: recording, partials, then stopped.
    let states: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Engine(EngineEvent::State(s)) => Some(*s),
            _ => None,
        })
        .collect();
    assert_eq!(states.first(), Some(&MeetingState::Recording));
    assert_eq!(states.last(), Some(&MeetingState::Stopped));
    assert!(events.iter().any(|e| matches!(e, SessionEvent::Engine(EngineEvent::Segment(s)) if !s.is_final)));

    // Saved last, after Finished: duration and status, ready for processing.
    let Some(SessionEvent::Saved(info)) = events.last() else { panic!("ends with Saved: {:?}", events.last()) };
    assert!(matches!(events[events.len() - 2], SessionEvent::Engine(EngineEvent::Finished(_))));
    assert_eq!(info.id, id);
    assert_eq!(info.status, Status::Processing);
    assert!((118_000..=118_500).contains(&info.duration_ms), "{}", info.duration_ms);
    assert_eq!(store.list().unwrap()[0], *info);

    // Each kept final is a line of live.ndjson and a row of the index, in order.
    let kept = committed(&events);
    assert!(kept.len() >= 10, "{} finals", kept.len());
    assert!(kept.iter().any(|s| s.source == Source::Me) && kept.iter().any(|s| s.source == Source::Them));
    let folder = store.folder(&id);
    let lines = std::fs::read_to_string(folder.join("live.ndjson")).unwrap();
    assert_eq!(lines.lines().count(), kept.len());
    let stored = store.segments(&id, TranscriptKind::Live).unwrap();
    let want: Vec<_> = kept.iter().map(|s| (s.source, s.start_ms, s.end_ms, s.text.clone())).collect();
    let got: Vec<_> = stored.iter().map(|s| (s.source, s.start_ms, s.end_ms, s.text.clone())).collect();
    assert_eq!(got, want);
    assert!(stored.iter().all(|s| s.speaker.is_none()));
    assert!(got.iter().any(|(_, _, _, text)| text.contains("December first")));

    // The recording is in the meeting's folder, the meeting's length.
    let rec = recap_engine::Recording::read(&folder.join("audio.ogg")).unwrap();
    let ms = rec.them.len() as u64 * 1000 / recap_engine::SAMPLE_RATE as u64;
    assert!(ms.abs_diff(info.duration_ms) < 200, "{ms} ms recorded");

    // The index can be rebuilt from the folder alone.
    drop(store);
    for file in ["app.db", "app.db-wal", "app.db-shm"] {
        let _ = std::fs::remove_file(dir.path().join(file));
    }
    assert!(!dir.path().join("app.db").exists());
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(reopened.segments(&id, TranscriptKind::Live).unwrap(), stored);
    assert_eq!(reopened.list().unwrap()[0].status, Status::Processing);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paused_meeting_stays_one_meeting_with_its_pause_saved() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (feed, source) = FrameFeed::new();
    let mut live = session::start(Some(store.clone()), config(false), Box::new(source), scripted(), None);

    feed.push_silence(Duration::from_secs(2));
    wait_for(&mut live.events, |e| matches!(e, SessionEvent::Engine(EngineEvent::Elapsed(2_000)))).await;
    live.control.pause();
    wait_for(&mut live.events, |e| matches!(e, SessionEvent::Engine(EngineEvent::State(MeetingState::Paused)))).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    live.control.resume();
    wait_for(&mut live.events, |e| matches!(e, SessionEvent::Engine(EngineEvent::State(MeetingState::Recording)))).await;
    feed.push_silence(Duration::from_secs(1));
    live.control.stop();
    let events = drain(&mut live.events).await;
    drop(feed);

    let Some(SessionEvent::Saved(info)) = events.last() else { panic!("ends with Saved") };
    assert_eq!(store.list().unwrap().len(), 1, "one meeting");
    let pauses: Vec<PauseMark> =
        serde_json::from_slice(&std::fs::read(store.folder(&info.id).join("pauses.json")).unwrap()).unwrap();
    assert_eq!(pauses.len(), 1);
    assert_eq!(pauses[0].at_ms, 2_000);
    assert!(pauses[0].resumed_ms > pauses[0].paused_ms);
}

#[tokio::test(flavor = "multi_thread")]
async fn notices_from_the_provider_job_reach_the_ui() {
    let (notices, rx) = session::notices();
    let (feed, source) = FrameFeed::new();
    let mut live = session::start(None, config(false), Box::new(source), scripted(), Some(rx));
    assert!(live.id.is_none(), "no store, nothing saved");
    notices("Switched to Deepgram".into());
    wait_for(&mut live.events, |e| matches!(e, SessionEvent::Notice(n) if n == "Switched to Deepgram")).await;
    drop(feed);
    let events = drain(&mut live.events).await;
    assert!(matches!(events.last(), Some(SessionEvent::Engine(EngineEvent::Finished(_)))));
}

async fn wait_for(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<SessionEvent>,
    want: impl Fn(&SessionEvent) -> bool,
) {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("the awaited event arrives")
            .expect("the session is still going");
        if want(&event) {
            return;
        }
    }
}
