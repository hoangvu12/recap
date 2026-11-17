//! Seam 1 after Stop: a meeting recorded from the WAV source through the
//! real live meeting and store, then processed with a fake batch STT that
//! replays the recorded ElevenLabs answers (tests/fixtures/stt_*.json, from
//! `processing_real.rs`). What the UI is told and what is left on disk.

mod recorded;

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use recap::processing::{
    Job, Processing, ProcessingConfig, ProcessingEvent, Progress, STT_ME, STT_THEM, Step, StepId, StepStatus,
};
use recap_engine::{
    BatchRequest, BatchStt, BatchTranscript, BoxFuture, Capabilities, ProviderError, ProviderErrorKind, Source,
};
use recap_storage::{MeetingId, Status, Store, TranscriptKind};

fn fixture(name: &str) -> BatchTranscript {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
}

/// What the fake was asked: the side (diarized = Them), how much audio, the keyterms.
#[derive(Clone, Debug)]
struct Asked {
    diarize: bool,
    samples: usize,
    keyterms: Vec<String>,
}

/// Answers like ElevenLabs did on the test meeting; can fail a side once.
#[derive(Clone, Default)]
struct FakeBatch {
    asked: Arc<Mutex<Vec<Asked>>>,
    /// Fail the next request for this side (diarize = Them) with this error.
    fail: Arc<Mutex<Option<(bool, ProviderError)>>>,
}

impl FakeBatch {
    fn failing(them: bool, error: ProviderError) -> Self {
        let fake = Self::default();
        *fake.fail.lock().unwrap() = Some((them, error));
        fake
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

impl BatchStt for FakeBatch {
    fn name(&self) -> &str {
        "Fake"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { batch: true, diarize: true, keyterms: true, word_timestamps: true, live: false }
    }

    fn transcribe(&self, request: BatchRequest) -> BoxFuture<'static, Result<BatchTranscript, ProviderError>> {
        self.asked.lock().unwrap().push(Asked {
            diarize: request.diarize,
            samples: request.samples.len(),
            keyterms: request.keyterms.clone(),
        });
        let mut fail = self.fail.lock().unwrap();
        let outcome = match fail.take() {
            Some((them, error)) if them == request.diarize => Err(error),
            other => {
                *fail = other;
                Ok(fixture(if request.diarize { STT_THEM } else { STT_ME }))
            }
        };
        Box::pin(async move { outcome })
    }
}

fn config() -> ProcessingConfig {
    ProcessingConfig { keyterms: vec!["Priya".into(), "David".into()], my_name: "Tom".into(), language: None }
}

/// Process `id` to the end; every event.
async fn process(processing: &Processing, store: &Store, id: &MeetingId) -> Vec<ProcessingEvent> {
    let mut events = processing.start(store.clone(), id.clone(), config());
    let mut out = Vec::new();
    while let Some(event) = events.recv().await {
        out.push(event);
    }
    out
}

fn standard(fake: &FakeBatch) -> Processing {
    Processing::standard(Ok(Arc::new(fake.clone()) as Arc<dyn BatchStt>))
}

fn statuses(progress: &Progress) -> Vec<(StepId, StepStatus)> {
    progress.steps.iter().map(|s| (s.id, s.status)).collect()
}

fn last_progress(events: &[ProcessingEvent]) -> Progress {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            ProcessingEvent::Progress(p) => Some(p.clone()),
            _ => None,
        })
        .expect("progress was told")
}

#[tokio::test(flavor = "multi_thread")]
async fn after_stop_the_recording_becomes_the_refined_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let info = recorded::record(&store).await;
    assert_eq!(info.status, Status::Processing);
    let fake = FakeBatch::default();
    let events = process(&standard(&fake), &store, &info.id).await;

    // Both sides went up at once, the whole recording each: Them diarized.
    let mut asked = fake.asked();
    asked.sort_by_key(|a| !a.diarize);
    assert_eq!(asked.iter().map(|a| a.diarize).collect::<Vec<_>>(), [true, false]);
    for a in &asked {
        let ms = a.samples as u64 * 1000 / 16_000;
        assert!(ms.abs_diff(info.duration_ms) < 200, "{ms} ms of audio");
        assert_eq!(a.keyterms, ["Priya", "David"]);
    }

    // The card: Transcribing runs, then each step is done; the meeting is done.
    let progress: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            ProcessingEvent::Progress(p) => Some(statuses(p)),
            _ => None,
        })
        .collect();
    use StepId::*;
    use StepStatus::*;
    assert_eq!(progress[0], [(Transcribing, Pending), (NamingSpeakers, Pending), (WritingSummary, Pending)]);
    assert_eq!(progress[1], [(Transcribing, Running), (NamingSpeakers, Pending), (WritingSummary, Pending)]);
    assert_eq!(progress[2], [(Transcribing, Done), (NamingSpeakers, Pending), (WritingSummary, Pending)]);
    assert_eq!(progress.last().unwrap(), &[(Transcribing, Done), (NamingSpeakers, Done), (WritingSummary, Done)]);
    assert!(matches!(events[3], ProcessingEvent::StepDone(Transcribing)), "{:?}", events[3]);
    let Some(ProcessingEvent::Finished(done)) = events.last() else { panic!("finished: {:?}", events.last()) };
    assert_eq!(done.status, Status::Done);
    assert!(last_progress(&events).step(Transcribing).unwrap().took_ms < 30_000);

    // The refined transcript: Them's three voices take turns as the script
    // has it; every Me line was an echo (the WAV's channels are the same).
    let refined = store.segments(&info.id, TranscriptKind::Refined).unwrap();
    assert!(refined.iter().all(|s| s.source == Source::Them), "echoes dropped");
    let voice = |line: &str| match line {
        "Mark" => "S1",
        "Zira" => "S2",
        _ => "S3",
    };
    let script = std::fs::read_to_string(Path::new(recorded::WAV).with_file_name("meeting_tts_script.txt")).unwrap();
    let want: Vec<&str> = script.lines().filter_map(|l| l.split_once('|')).map(|(v, _)| voice(v)).collect();
    let got: Vec<&str> = refined.iter().map(|s| s.speaker.as_deref().unwrap()).collect();
    assert_eq!(got, want, "one turn per line of the script");
    assert!(refined.windows(2).all(|w| w[0].start_ms <= w[1].start_ms));
    assert!(refined[0].text.starts_with("Okay, let's get started."), "{}", refined[0].text);
    assert!(refined.iter().any(|s| s.text.contains("soft launch in India")));

    // It replaces the live transcript, with the speakers listed.
    assert_eq!(store.transcript(&info.id).unwrap(), refined);
    let ids: Vec<_> = done.speakers.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["S1", "S2", "S3"]);
    assert_eq!(store.list().unwrap()[0], *done);

    // On disk: both answers and the progress, all done.
    let folder = store.folder(&info.id);
    assert_eq!(fixture(STT_THEM), serde_json::from_slice(&std::fs::read(folder.join(STT_THEM)).unwrap()).unwrap());
    assert!(folder.join(STT_ME).exists() && folder.join("refined.ndjson").exists());
    assert!(Progress::read(&folder).finished());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_step_says_why_and_retry_resumes_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    let fake = FakeBatch::failing(false, ProviderError::new(ProviderErrorKind::Quota, "ElevenLabs is out of credit."));
    let processing = standard(&fake);

    let events = process(&processing, &store, &id).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Stopped(e)) if e == "ElevenLabs is out of credit."));
    let progress = last_progress(&events);
    let step = progress.step(StepId::Transcribing).unwrap();
    assert_eq!((step.status, step.error.as_str()), (StepStatus::Error, "ElevenLabs is out of credit."));
    assert_eq!(progress.failed().unwrap().id, StepId::Transcribing);
    assert_eq!(
        statuses(&progress)[1..],
        [(StepId::NamingSpeakers, StepStatus::Pending), (StepId::WritingSummary, StepStatus::Pending)]
    );
    assert_eq!(Progress::read(&store.folder(&id)), progress, "kept on disk");
    assert_eq!(store.meeting(&id).unwrap().unwrap().info.status, Status::Processing, "unfinished");
    assert!(store.segments(&id, TranscriptKind::Refined).unwrap().is_empty());
    // What did work is kept.
    assert!(store.folder(&id).join(STT_THEM).exists());

    // Retry: only the side that failed goes up again.
    let events = process(&processing, &store, &id).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    let sides: Vec<bool> = fake.asked().iter().map(|a| a.diarize).collect();
    assert_eq!(sides.iter().filter(|&&them| them).count(), 1, "Them once: {sides:?}");
    assert_eq!(sides.iter().filter(|&&them| !them).count(), 2, "Me again");
    assert_eq!(store.meeting(&id).unwrap().unwrap().info.status, Status::Done);
    assert!(!store.segments(&id, TranscriptKind::Refined).unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_meeting_killed_while_transcribing_recovers_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let store = Store::open(dir.path()).unwrap();
        let id = recorded::stopped(&store);
        // Killed mid-step: Them's answer was kept, Me's never came.
        let folder = store.folder(&id);
        std::fs::write(folder.join(STT_THEM), serde_json::to_vec(&fixture(STT_THEM)).unwrap()).unwrap();
        let mut progress = Progress::default();
        progress.steps[0].status = StepStatus::Running;
        std::fs::write(folder.join("processing.json"), serde_json::to_vec(&progress).unwrap()).unwrap();
        id
    };

    // Restart: the index is rebuilt too; the meeting is unfinished.
    for file in ["app.db", "app.db-wal", "app.db-shm"] {
        let _ = std::fs::remove_file(dir.path().join(file));
    }
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.list().unwrap()[0].status, Status::Processing, "Home offers Recover");
    assert_eq!(Progress::read(&store.folder(&id)).steps[0].status, StepStatus::Running);

    // Recover: the step starts again and asks only for what is missing.
    let fake = FakeBatch::default();
    let events = process(&standard(&fake), &store, &id).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    assert_eq!(fake.asked().iter().map(|a| a.diarize).collect::<Vec<_>>(), [false], "Me only");
    assert_eq!(store.list().unwrap()[0].status, Status::Done);
    assert_eq!(store.segments(&id, TranscriptKind::Refined).unwrap().len(), 12);
}

/// A step in a later slot: records that it ran.
struct Probe(StepId, Arc<Mutex<Vec<MeetingId>>>);

impl Step for Probe {
    fn id(&self) -> StepId {
        self.0
    }

    fn run(&self, job: Job) -> BoxFuture<'static, Result<(), String>> {
        self.1.lock().unwrap().push(job.id.clone());
        let refined = job.store.segments(&job.id, TranscriptKind::Refined);
        Box::pin(async move {
            match refined {
                Ok(segments) if !segments.is_empty() => Ok(()),
                other => Err(format!("no refined transcript before naming: {other:?}")),
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn steps_done_before_are_not_run_again_and_later_slots_take_their_steps() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    let fake = FakeBatch::default();
    let ran = Arc::new(Mutex::new(Vec::new()));
    let processing = standard(&fake).with(Arc::new(Probe(StepId::NamingSpeakers, ran.clone())));

    let events = process(&processing, &store, &id).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))), "{:?}", events.last());
    assert_eq!(*ran.lock().unwrap(), std::slice::from_ref(&id), "the naming slot ran, after Transcribing");
    assert_eq!(fake.asked().len(), 2);

    // Recover a done meeting (Regenerate later): nothing runs again.
    let events = process(&processing, &store, &id).await;
    assert!(matches!(events.last(), Some(ProcessingEvent::Finished(_))));
    assert_eq!(fake.asked().len(), 2);
    assert_eq!(ran.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_transcript_provider_the_step_fails_with_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let id = recorded::stopped(&store);
    let none = Err(ProviderError::new(ProviderErrorKind::Other, "Transcript has no provider"));
    let events = process(&Processing::standard(none), &store, &id).await;
    let progress = last_progress(&events);
    assert_eq!(progress.failed().unwrap().error, "Transcript has no provider");

    // A meeting without its recording says so.
    let bare = store.create(recorded::STARTED + 1).unwrap().id;
    let events = process(&standard(&FakeBatch::default()), &store, &bare).await;
    assert_eq!(last_progress(&events).failed().unwrap().error, "This meeting has no recording.");
    assert_eq!(
        store.meeting(&bare).unwrap().unwrap().info.status,
        Status::Processing,
        "cut off while recording: now unfinished"
    );
}

#[test]
fn the_card_reads_the_progress_view() {
    let mut progress = Progress::default();
    progress.steps[0].status = StepStatus::Done;
    progress.steps[0].took_ms = 11_500;
    progress.steps[1].status = StepStatus::Error;
    progress.steps[1].error = "Claude CLI isn't signed in.".into();
    assert_eq!(
        progress.view(&MeetingId::from("20261116-100000")),
        serde_json::json!({
            "id": "20261116-100000",
            "steps": [
                { "id": "transcribing", "label": "Transcribing", "status": "done", "took_ms": 11500, "error": "" },
                { "id": "naming_speakers", "label": "Naming speakers", "status": "error", "took_ms": 0, "error": "Claude CLI isn't signed in." },
                { "id": "writing_summary", "label": "Writing summary", "status": "pending", "took_ms": 0, "error": "" },
            ],
        })
    );
}
