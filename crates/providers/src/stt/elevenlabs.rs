//! ElevenLabs Scribe: live captions (`scribe_v3_live`, a websocket) and the
//! after-Stop transcript (`scribe_v3`, an upload). DESIGN §4.2, §4.3; the
//! parameters are the tested ones in `reference/` and
//! `docs/research/stt_results.md`.
//!
//! - [`live`]: one websocket per side. Partials replace each other, commits
//!   (turn prediction) are finals timed by their words. A dropped socket or
//!   a session time limit reconnects with backoff and replays the audio
//!   since the last final, with the last finals as `previous_text`. Silence
//!   keeps an idle socket open. Auth and quota errors end the session typed,
//!   so the job falls back to its backup.
//! - [`batch`]: one side as a WAV upload, words in ms, diarized speakers as
//!   `S1`… in order of first appearance.
//! - Test button: open a realtime session and wait for `session_started`.

mod batch;
mod live;

use std::sync::Arc;
use std::time::Duration;

use recap_engine::{
    AudioFrame, BatchRequest, BatchStt, BatchTranscript, BoxFuture, Capabilities, LiveEvent, LiveOptions, LiveStt,
    ProviderError, ProviderErrorKind, Source,
};
use tokio::sync::mpsc;

use crate::{
    provider::{Provider, ProviderSettings},
    registry::{Family, Fields, Group, Kind, Preset},
};

pub const KIND: Kind = Kind { id: "elevenlabs", family: Family::Stt, capabilities: CAPABILITIES, build };

const CAPABILITIES: Capabilities =
    Capabilities { live: true, batch: true, diarize: true, keyterms: true, word_timestamps: true };

pub const PRESET: Preset = Preset {
    fields: Fields { key: true, model: false, base_url: false, command: false, name: false },
    key_hint: "sk_…",
    ..Preset::new("elevenlabs", "ElevenLabs", "elevenlabs", Group::Speech, "elevenlabs")
};

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(ElevenLabs::new(&settings.name, settings.key.as_deref().unwrap_or(""), Config::default()))
}

/// Where the adapter talks to and how patient it is. Tests point it at
/// local fakes and shorten the waits.
#[derive(Clone, Debug)]
pub struct Config {
    /// The realtime websocket's base, `wss://api.elevenlabs.io`.
    pub realtime: String,
    /// The HTTP API's base, `https://api.elevenlabs.io`.
    pub api: String,
    /// The first reconnect waits this long, then twice as long each time
    /// (up to 8 s).
    pub backoff: Duration,
    /// Reconnects in a row without hearing anything before the session
    /// gives up with a network error (and the job moves to its backup).
    pub retries: u32,
    /// With no audio for this long, send 100 ms of silence: ElevenLabs
    /// closes a session after 15 s without client messages.
    pub keepalive: Duration,
    /// After the audio closes, how long to wait for the last commit.
    pub flush: Duration,
    /// Connecting (TCP, TLS, upgrade) may take this long.
    pub connect_timeout: Duration,
    /// The batch upload may take this long (a 3 h side is ~350 MB).
    pub batch_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            realtime: "wss://api.elevenlabs.io".into(),
            api: "https://api.elevenlabs.io".into(),
            backoff: Duration::from_millis(500),
            retries: 5,
            keepalive: Duration::from_secs(5),
            flush: Duration::from_secs(3),
            connect_timeout: Duration::from_secs(10),
            batch_timeout: Duration::from_secs(20 * 60),
        }
    }
}

/// The ElevenLabs provider: live captions, the after-Stop transcript and
/// the Test button.
#[derive(Clone)]
pub struct ElevenLabs(Arc<Inner>);

struct Inner {
    name: String,
    key: String,
    config: Config,
}

impl ElevenLabs {
    pub fn new(name: &str, key: &str, config: Config) -> Self {
        Self(Arc::new(Inner { name: name.to_owned(), key: key.to_owned(), config }))
    }
}

impl Provider for ElevenLabs {
    fn live(&self) -> Option<Arc<dyn LiveStt>> {
        Some(Arc::new(self.clone()))
    }

    fn batch(&self) -> Option<Arc<dyn BatchStt>> {
        Some(Arc::new(self.clone()))
    }

    fn test(&self) -> BoxFuture<'static, Result<(), ProviderError>> {
        let inner = self.0.clone();
        Box::pin(async move { live::check(&inner).await })
    }
}

impl LiveStt for ElevenLabs {
    fn name(&self) -> &str {
        &self.0.name
    }

    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn run(
        &self,
        _source: Source,
        options: LiveOptions,
        audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        let inner = self.0.clone();
        Box::pin(async move { live::run(inner, options, audio, events).await })
    }
}

impl BatchStt for ElevenLabs {
    fn name(&self) -> &str {
        &self.0.name
    }

    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn transcribe(&self, request: BatchRequest) -> BoxFuture<'static, Result<BatchTranscript, ProviderError>> {
        let inner = self.0.clone();
        Box::pin(async move { batch::transcribe(&inner, request).await })
    }
}

/// An HTTP error status (the upload, or a refused websocket upgrade) as a
/// typed error. ElevenLabs answers an exhausted quota with 401 and
/// `detail.status = "quota_exceeded"`.
fn http_error(status: u16, body: &str) -> ProviderError {
    let detail = serde_json::from_str::<serde_json::Value>(body).ok().map(|v| v["detail"].clone());
    let code = detail.as_ref().and_then(|d| d["status"].as_str()).unwrap_or("").to_owned();
    let message = detail
        .as_ref()
        .and_then(|d| d["message"].as_str().or_else(|| d.as_str()))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("HTTP {status}"));
    let kind = match (status, code.as_str()) {
        (_, "quota_exceeded") => ProviderErrorKind::Quota,
        (401 | 403, _) => ProviderErrorKind::Auth,
        (402 | 429, _) => ProviderErrorKind::Quota,
        (408 | 500..=599, _) => ProviderErrorKind::Network,
        _ => ProviderErrorKind::Protocol,
    };
    let message = match kind {
        ProviderErrorKind::Auth => format!("ElevenLabs refused the key: {message}"),
        ProviderErrorKind::Quota => format!("ElevenLabs quota: {message}"),
        _ => format!("ElevenLabs: {message}"),
    };
    ProviderError::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_errors_map_to_kinds() {
        assert_eq!(http_error(401, r#"{"detail":{"status":"quota_exceeded"}}"#).kind, ProviderErrorKind::Quota);
        assert_eq!(http_error(401, "").kind, ProviderErrorKind::Auth);
        assert_eq!(http_error(429, "").kind, ProviderErrorKind::Quota);
        assert_eq!(http_error(502, "").kind, ProviderErrorKind::Network);
        assert_eq!(http_error(422, "").kind, ProviderErrorKind::Protocol);
    }
}
