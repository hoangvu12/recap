//! Deepgram Nova-3, live and pre-recorded: the backup speech engine
//! (DESIGN §4.2). The parameters are the ones the earlier Python app used
//! (`live_captions.py`, `transcribe_file.py`); the message shapes are
//! Deepgram's documented ones (no key to record a real session with).
//!
//! - [`live`]: one websocket per side, audio as binary 16 kHz linear16.
//!   Interim results are partials; finals of one utterance join until
//!   `speech_final` or `UtteranceEnd`. A dropped socket or a server close
//!   (`1011 NET-…`) reconnects with backoff and replays the audio since the
//!   last final. `KeepAlive` while no audio comes. `CloseStream` at the end
//!   flushes the last words. A refused key or no credit ends the session
//!   typed, so the job falls back.
//! - [`batch`]: one side as a WAV upload, punctuated words in ms, diarized
//!   speakers as `S1`… in order of first appearance.
//! - Test button: open a live stream (no audio) and close it.

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

pub const KIND: Kind = Kind { id: "deepgram", family: Family::Stt, capabilities: CAPABILITIES, build };

const CAPABILITIES: Capabilities =
    Capabilities { live: true, batch: true, diarize: true, keyterms: true, word_timestamps: true };

pub const PRESET: Preset = Preset {
    fields: Fields { key: true, model: false, base_url: false, command: false, name: false },
    ..Preset::new("deepgram", "Deepgram", "deepgram", Group::Speech, "deepgram")
};

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(Deepgram::new(&settings.name, settings.key.as_deref().unwrap_or(""), Config::default()))
}

/// Where the adapter talks to and how patient it is. Tests point it at
/// local fakes and shorten the waits.
#[derive(Clone, Debug)]
pub struct Config {
    /// The live websocket's base, `wss://api.deepgram.com`.
    pub live: String,
    /// The HTTP API's base, `https://api.deepgram.com`.
    pub api: String,
    /// The first reconnect waits this long, then twice as long each time
    /// (up to 8 s).
    pub backoff: Duration,
    /// Reconnects in a row without hearing anything before the session
    /// gives up with a network error.
    pub retries: u32,
    /// With no audio for this long, send `KeepAlive`: Deepgram closes a
    /// stream after 10 s without data (it asks for one every 3-5 s).
    pub keepalive: Duration,
    /// After `CloseStream`, how long to wait for the last results.
    pub flush: Duration,
    pub connect_timeout: Duration,
    /// The pre-recorded upload may take this long.
    pub batch_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            live: "wss://api.deepgram.com".into(),
            api: "https://api.deepgram.com".into(),
            backoff: Duration::from_millis(500),
            retries: 5,
            keepalive: Duration::from_secs(4),
            flush: Duration::from_secs(3),
            connect_timeout: Duration::from_secs(10),
            batch_timeout: Duration::from_secs(20 * 60),
        }
    }
}

/// The Deepgram provider: live captions, the after-Stop transcript and the
/// Test button.
#[derive(Clone)]
pub struct Deepgram(Arc<Inner>);

struct Inner {
    name: String,
    key: String,
    config: Config,
}

impl Deepgram {
    pub fn new(name: &str, key: &str, config: Config) -> Self {
        Self(Arc::new(Inner { name: name.to_owned(), key: key.to_owned(), config }))
    }
}

impl Provider for Deepgram {
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

impl LiveStt for Deepgram {
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

impl BatchStt for Deepgram {
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
/// typed error. Deepgram's body is `{err_code, err_msg, request_id}`; no
/// credit is `402 ASR_PAYMENT_REQUIRED`.
fn http_error(status: u16, body: &str) -> ProviderError {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let field = |name: &str| parsed.as_ref().and_then(|v| v[name].as_str()).unwrap_or("").to_owned();
    let (code, message) = (field("err_code"), field("err_msg"));
    let message = if message.is_empty() { format!("HTTP {status}") } else { message };
    let kind = match (status, code.as_str()) {
        (_, "ASR_PAYMENT_REQUIRED") => ProviderErrorKind::Quota,
        (401 | 403, _) => ProviderErrorKind::Auth,
        (402 | 429, _) => ProviderErrorKind::Quota,
        (408 | 500..=599, _) => ProviderErrorKind::Network,
        _ => ProviderErrorKind::Protocol,
    };
    let message = match kind {
        ProviderErrorKind::Auth => format!("Deepgram refused the key: {message}"),
        ProviderErrorKind::Quota => format!("Deepgram quota: {message}"),
        _ => format!("Deepgram: {message}"),
    };
    ProviderError::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_errors_map_to_kinds() {
        assert_eq!(http_error(402, r#"{"err_code":"ASR_PAYMENT_REQUIRED"}"#).kind, ProviderErrorKind::Quota);
        assert_eq!(http_error(401, "").kind, ProviderErrorKind::Auth);
        assert_eq!(http_error(429, "").kind, ProviderErrorKind::Quota);
        assert_eq!(http_error(502, "").kind, ProviderErrorKind::Network);
        assert_eq!(http_error(400, "").kind, ProviderErrorKind::Protocol);
        assert_eq!(http_error(401, r#"{"err_msg":"Invalid credentials."}"#).message, "Deepgram refused the key: Invalid credentials.");
    }
}
