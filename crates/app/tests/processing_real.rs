//! Transcribing against the real ElevenLabs batch API: ignored unless asked
//! for, and skipped without `ELEVENLABS_API_KEY`.
//!
//! `cargo test -p recap --test processing_real -- --ignored --nocapture`
//!
//! With `RECAP_WRITE_FIXTURES=1` it also keeps both sides' batch transcripts
//! as `tests/fixtures/stt_{them,me}.json` (what `tests/processing.rs` replays).

mod recorded;

use std::{collections::BTreeSet, path::Path, sync::Arc};

use recap::processing::{Processing, ProcessingConfig, ProcessingEvent, STT_ME, STT_THEM};
use recap_engine::BatchStt;
use recap_providers::elevenlabs::{Config, ElevenLabs};
use recap_storage::{Status, Store, TranscriptKind};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "real API: needs ELEVENLABS_API_KEY"]
async fn the_test_meeting_has_three_speakers_on_the_system_side() {
    let Some(key) = std::env::var("ELEVENLABS_API_KEY").ok().filter(|k| !k.trim().is_empty()) else {
        eprintln!("ELEVENLABS_API_KEY is not set: skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::record(&store).await.id;

    let batch: Arc<dyn BatchStt> = Arc::new(ElevenLabs::new("ElevenLabs", key.trim(), Config::default()));
    let config =
        ProcessingConfig { keyterms: vec!["Priya".into(), "David".into(), "Tom".into()], my_name: String::new() };
    let mut events = Processing::standard(Ok(batch)).start(store.clone(), id.clone(), config);
    let mut last = None;
    while let Some(event) = events.recv().await {
        if let ProcessingEvent::Progress(p) = &event {
            eprintln!("{:?}", p.steps.iter().map(|s| (s.id.as_str(), s.status, s.took_ms)).collect::<Vec<_>>());
        }
        last = Some(event);
    }
    let Some(ProcessingEvent::Finished(info)) = last else { panic!("finished: {last:?}") };
    assert_eq!(info.status, Status::Done);

    let refined = store.segments(&id, TranscriptKind::Refined).unwrap();
    for s in &refined {
        eprintln!("[{:>6}] {:<4} {}", s.start_ms, s.speaker.as_deref().unwrap_or("-"), s.text);
    }
    let voices: BTreeSet<_> = refined.iter().filter_map(|s| s.speaker.as_deref()).filter(|s| *s != "me").collect();
    assert_eq!(voices, BTreeSet::from(["S1", "S2", "S3"]), "three voices on the system side");
    // Both channels of the test WAV are the same: everything on the mic is an echo.
    assert!(refined.iter().all(|s| s.speaker.as_deref() != Some("me")), "echoes dropped");
    let text = refined.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
    assert!(text.contains("december 1") && text.contains("india"), "{text}");

    if std::env::var_os("RECAP_WRITE_FIXTURES").is_some() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        std::fs::create_dir_all(&fixtures).unwrap();
        for file in [STT_THEM, STT_ME] {
            std::fs::copy(store.folder(&id).join(file), fixtures.join(file)).unwrap();
        }
        eprintln!("fixtures written to {}", fixtures.display());
    }
}
