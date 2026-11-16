//! A job's main provider failing on quota, auth or the network hands the job
//! to its backup; any other error is the job's own (GLOSSARY: Job).

use std::sync::{Arc, Mutex};

use recap_engine::{
    AudioFrame, BatchRequest, BatchStt, BatchTranscript, BoxFuture, Capabilities, Fallback, FallbackNotice,
    LiveEvent, LiveOptions, LiveStt, Llm, ProviderError, ProviderErrorKind, Source, Word,
};
use tokio::sync::mpsc;

fn error(kind: ProviderErrorKind) -> ProviderError {
    ProviderError::new(kind, format!("{kind:?}"))
}

/// An AI provider that answers `answer`, counting its calls.
struct FakeLlm {
    name: &'static str,
    answer: Result<serde_json::Value, ProviderError>,
    calls: Arc<Mutex<u32>>,
}

impl FakeLlm {
    fn new(name: &'static str, answer: Result<serde_json::Value, ProviderError>) -> Arc<Self> {
        Arc::new(Self { name, answer, calls: Arc::default() })
    }
    fn calls(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
}

impl Llm for FakeLlm {
    fn name(&self) -> &str {
        self.name
    }
    fn complete_json(
        &self,
        _prompt: String,
        _schema: serde_json::Value,
    ) -> BoxFuture<'static, Result<serde_json::Value, ProviderError>> {
        *self.calls.lock().unwrap() += 1;
        let answer = self.answer.clone();
        Box::pin(async move { answer })
    }
}

fn notices() -> (Arc<Mutex<Vec<FallbackNotice>>>, impl Fn(FallbackNotice) + Send + Sync + 'static) {
    let seen: Arc<Mutex<Vec<FallbackNotice>>> = Arc::default();
    let sink = seen.clone();
    (seen, move |notice| sink.lock().unwrap().push(notice))
}

#[tokio::test]
async fn the_main_provider_does_the_job_when_it_works() {
    let main = FakeLlm::new("Claude Code", Ok(serde_json::json!({ "title": "main" })));
    let backup = FakeLlm::new("OpenRouter", Ok(serde_json::json!({ "title": "backup" })));
    let job = Fallback::new(main.clone() as Arc<dyn Llm>, Some(backup.clone() as Arc<dyn Llm>));
    let answer = job.complete_json("p".into(), serde_json::json!({})).await.unwrap();
    assert_eq!(answer["title"], "main");
    assert_eq!(backup.calls(), 0);
}

#[tokio::test]
async fn quota_auth_and_network_errors_hand_the_job_to_the_backup() {
    for kind in [ProviderErrorKind::Quota, ProviderErrorKind::Auth, ProviderErrorKind::Network] {
        let main = FakeLlm::new("Claude Code", Err(error(kind)));
        let backup = FakeLlm::new("OpenRouter", Ok(serde_json::json!({ "title": "backup" })));
        let (seen, on_switch) = notices();
        let job = Fallback::new(main.clone() as Arc<dyn Llm>, Some(backup.clone() as Arc<dyn Llm>)).on_switch(on_switch);
        let answer = job.complete_json("p".into(), serde_json::json!({})).await.unwrap();
        assert_eq!(answer["title"], "backup", "{kind:?}");
        assert_eq!((main.calls(), backup.calls()), (1, 1));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!((seen[0].from.as_str(), seen[0].to.as_str()), ("Claude Code", "OpenRouter"));
        assert_eq!(seen[0].error.kind, kind);
    }
}

#[tokio::test]
async fn other_errors_are_the_jobs_own() {
    let main = FakeLlm::new("Claude Code", Err(error(ProviderErrorKind::Protocol)));
    let backup = FakeLlm::new("OpenRouter", Ok(serde_json::json!({})));
    let job = Fallback::new(main as Arc<dyn Llm>, Some(backup.clone() as Arc<dyn Llm>));
    let failed = job.complete_json("p".into(), serde_json::json!({})).await.unwrap_err();
    assert_eq!(failed.kind, ProviderErrorKind::Protocol);
    assert_eq!(backup.calls(), 0);
}

#[tokio::test]
async fn without_a_backup_the_mains_error_stands() {
    let main = FakeLlm::new("Claude Code", Err(error(ProviderErrorKind::Quota)));
    let job = Fallback::new(main as Arc<dyn Llm>, None);
    assert_eq!(job.complete_json("p".into(), serde_json::json!({})).await.unwrap_err().kind, ProviderErrorKind::Quota);
}

#[tokio::test]
async fn when_the_backup_fails_too_its_error_is_reported() {
    let main = FakeLlm::new("Claude Code", Err(error(ProviderErrorKind::Auth)));
    let backup = FakeLlm::new("OpenRouter", Err(error(ProviderErrorKind::Network)));
    let job = Fallback::new(main as Arc<dyn Llm>, Some(backup as Arc<dyn Llm>));
    assert_eq!(job.complete_json("p".into(), serde_json::json!({})).await.unwrap_err().kind, ProviderErrorKind::Network);
}

/// A batch provider that answers one word, or fails.
struct FakeBatch(&'static str, Option<ProviderErrorKind>);

impl BatchStt for FakeBatch {
    fn name(&self) -> &str {
        self.0
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { batch: true, ..Default::default() }
    }
    fn transcribe(&self, _request: BatchRequest) -> BoxFuture<'static, Result<BatchTranscript, ProviderError>> {
        let (name, fails) = (self.0, self.1);
        Box::pin(async move {
            match fails {
                Some(kind) => Err(error(kind)),
                None => Ok(BatchTranscript {
                    words: vec![Word { text: name.into(), start_ms: 0, end_ms: 100, speaker: None }],
                }),
            }
        })
    }
}

#[tokio::test]
async fn a_transcript_falls_back_the_same_way() {
    let job = Fallback::new(
        Arc::new(FakeBatch("ElevenLabs", Some(ProviderErrorKind::Quota))) as Arc<dyn BatchStt>,
        Some(Arc::new(FakeBatch("Deepgram", None)) as Arc<dyn BatchStt>),
    );
    let transcript = job.transcribe(BatchRequest::default()).await.unwrap();
    assert_eq!(transcript.words[0].text, "Deepgram");
    assert_eq!(BatchStt::name(&job), "ElevenLabs");
}

/// A live provider that captions each frame as a final ("<name> <start_ms>"),
/// and fails with `fails` once it has heard `fail_after` frames.
struct FakeLive {
    name: &'static str,
    fail_after: Option<(usize, ProviderErrorKind)>,
}

impl LiveStt for FakeLive {
    fn name(&self) -> &str {
        self.name
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { live: true, ..Default::default() }
    }
    fn run(
        &self,
        _source: Source,
        _options: LiveOptions,
        mut audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let (name, fail_after) = (self.name, self.fail_after);
        Box::pin(async move {
            let mut heard = 0;
            while let Some(frame) = audio.recv().await {
                if let Some((after, kind)) = fail_after
                    && heard == after
                {
                    return Err(error(kind));
                }
                heard += 1;
                let text = format!("{name} {}", frame.start_ms);
                let _ = events.send(LiveEvent { text, is_final: true, start_ms: frame.start_ms, end_ms: frame.start_ms + 100 });
            }
            Ok(())
        })
    }
}

async fn caption(job: &dyn LiveStt, frames: u64) -> (Result<(), ProviderError>, Vec<String>) {
    let (audio_tx, audio) = mpsc::unbounded_channel();
    let (events, mut heard) = mpsc::unbounded_channel();
    for i in 0..frames {
        audio_tx.send(AudioFrame::silence(Source::Them, i * 100, 100)).unwrap();
    }
    drop(audio_tx);
    let result = job.run(Source::Them, LiveOptions::default(), audio, events).await;
    let mut texts = Vec::new();
    while let Ok(event) = heard.try_recv() {
        texts.push(event.text);
    }
    (result, texts)
}

#[tokio::test]
async fn live_captions_go_on_with_the_backup_from_where_the_main_failed() {
    let (seen, on_switch) = notices();
    let job = Fallback::new(
        Arc::new(FakeLive { name: "ElevenLabs", fail_after: Some((2, ProviderErrorKind::Quota)) }) as Arc<dyn LiveStt>,
        Some(Arc::new(FakeLive { name: "Deepgram", fail_after: None }) as Arc<dyn LiveStt>),
    )
    .on_switch(on_switch);
    let (result, texts) = caption(&job, 5).await;
    result.unwrap();
    assert_eq!(&texts[..2], ["ElevenLabs 0", "ElevenLabs 100"]);
    // The frame the main failed on may be lost; the rest reach the backup.
    assert_eq!(texts.last().unwrap(), "Deepgram 400");
    assert!(texts[2..].iter().all(|t| t.starts_with("Deepgram")));
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_live_error_that_does_not_fall_back_ends_the_session() {
    let job = Fallback::new(
        Arc::new(FakeLive { name: "ElevenLabs", fail_after: Some((1, ProviderErrorKind::Protocol)) }) as Arc<dyn LiveStt>,
        Some(Arc::new(FakeLive { name: "Deepgram", fail_after: None }) as Arc<dyn LiveStt>),
    );
    let (result, texts) = caption(&job, 5).await;
    assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Protocol);
    assert_eq!(texts, ["ElevenLabs 0"]);
}

#[tokio::test]
async fn live_captions_without_failure_stay_with_the_main() {
    let job = Fallback::new(
        Arc::new(FakeLive { name: "ElevenLabs", fail_after: None }) as Arc<dyn LiveStt>,
        Some(Arc::new(FakeLive { name: "Deepgram", fail_after: None }) as Arc<dyn LiveStt>),
    );
    let (result, texts) = caption(&job, 3).await;
    result.unwrap();
    assert_eq!(texts, ["ElevenLabs 0", "ElevenLabs 100", "ElevenLabs 200"]);
}
