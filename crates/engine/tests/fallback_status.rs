//! A live job's backup taking over is told as a status (the overlay names
//! the provider captioning now), and each provider's own statuses pass on.

use std::sync::Arc;

use recap_engine::{
    AudioFrame, BoxFuture, Capabilities, Fallback, LiveEvent, LiveOptions, LiveStatus, LiveStt, ProviderError,
    ProviderErrorKind, Source,
};
use tokio::sync::mpsc;

/// Reports Reconnecting once, then fails with `fails` (or ends well).
struct Blip {
    name: &'static str,
    fails: Option<ProviderErrorKind>,
}

impl LiveStt for Blip {
    fn name(&self) -> &str {
        self.name
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { live: true, ..Default::default() }
    }
    fn run(
        &self,
        source: Source,
        options: LiveOptions,
        audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let (status, _) = mpsc::unbounded_channel();
        self.run_reporting(source, options, audio, events, status)
    }
    fn run_reporting(
        &self,
        _source: Source,
        _options: LiveOptions,
        mut audio: mpsc::UnboundedReceiver<AudioFrame>,
        _events: mpsc::UnboundedSender<LiveEvent>,
        status: mpsc::UnboundedSender<LiveStatus>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let fails = self.fails;
        Box::pin(async move {
            let _ = status.send(LiveStatus::Reconnecting);
            if let Some(kind) = fails {
                return Err(ProviderError::new(kind, "no"));
            }
            while audio.recv().await.is_some() {}
            Ok(())
        })
    }
}

#[tokio::test]
async fn a_switch_to_the_backup_is_told_and_statuses_pass_on() {
    let job = Fallback::new(
        Arc::new(Blip { name: "ElevenLabs", fails: Some(ProviderErrorKind::Quota) }) as Arc<dyn LiveStt>,
        Some(Arc::new(Blip { name: "Deepgram", fails: None }) as Arc<dyn LiveStt>),
    );
    let (audio_tx, audio) = mpsc::unbounded_channel();
    let (events, _heard) = mpsc::unbounded_channel();
    let (status, mut statuses) = mpsc::unbounded_channel();
    drop(audio_tx);
    job.run_reporting(Source::Them, LiveOptions::default(), audio, events, status).await.unwrap();
    let mut seen = Vec::new();
    while let Ok(s) = statuses.try_recv() {
        seen.push(s);
    }
    assert_eq!(seen, [LiveStatus::Reconnecting, LiveStatus::Using("Deepgram".into()), LiveStatus::Reconnecting]);
}
