//! A stored, stopped meeting of the test WAV, as a live session leaves it:
//! recorded to audio.ogg, status processing, ready for the pipeline.

#![allow(dead_code)]

use std::{path::Path, sync::Arc, sync::OnceLock, time::Duration};

use recap::session::{self, SessionConfig, SessionEvent};
use recap_engine::{Script, ScriptedLiveStt, WavSource};
use recap_storage::{MeetingId, MeetingInfo, Status, Store};

pub const STARTED: u64 = 1_794_830_400_000;
pub const WAV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/meeting_tts.wav");

/// Replay the test WAV through a live session into `store` (unpaced, the
/// scripted live provider); the meeting once saved.
pub async fn record(store: &Store) -> MeetingInfo {
    let source = WavSource::open(Path::new(WAV)).unwrap();
    let live = Arc::new(ScriptedLiveStt::new(Script::test_meeting()));
    let config = SessionConfig { started_ms: STARTED, caption_me: false, ..Default::default() };
    let mut session = session::start(Some(store.clone()), config, Box::new(source), live, None);
    loop {
        let event = tokio::time::timeout(Duration::from_secs(60), session.events.recv())
            .await
            .expect("the session keeps talking")
            .expect("the session ends with Saved");
        if let SessionEvent::Saved(info) = event {
            return info;
        }
    }
}

/// The test meeting's recording, made once for every test of a binary.
fn audio() -> &'static (Vec<u8>, u64) {
    static AUDIO: OnceLock<(Vec<u8>, u64)> = OnceLock::new();
    AUDIO.get_or_init(|| {
        std::thread::spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
            runtime.block_on(async {
                let dir = tempfile::tempdir().unwrap();
                let store = Store::open(dir.path()).unwrap();
                let info = record(&store).await;
                (std::fs::read(store.folder(&info.id).join("audio.ogg")).unwrap(), info.duration_ms)
            })
        })
        .join()
        .unwrap()
    })
}

/// A meeting in `store` stopped with the test meeting's recording, as
/// [`record`] leaves it (without its live transcript).
pub fn stopped(store: &Store) -> MeetingId {
    let (ogg, duration_ms) = audio();
    let info = store.create(STARTED).unwrap();
    std::fs::write(store.folder(&info.id).join("audio.ogg"), ogg).unwrap();
    store
        .update(&info.id, |m| {
            m.status = Status::Processing;
            m.duration_ms = *duration_ms;
        })
        .unwrap();
    info.id
}
