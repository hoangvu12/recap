//! The provider seams (GLOSSARY: Provider, Job, Capability). STT adapters are
//! Rust code implementing [`LiveStt`] and/or [`BatchStt`]; AI providers sit
//! behind [`Llm`] (text in, JSON out). A job's main and backup are one
//! provider through [`crate::Fallback`]; the registry of provider kinds and
//! presets is `recap-providers`.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{AudioFrame, Source};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What an STT provider can do; pickers list only providers that can do a job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub live: bool,
    pub batch: bool,
    pub diarize: bool,
    pub keyterms: bool,
    pub word_timestamps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    /// Key missing, wrong or revoked: stop and surface it, try the backup.
    Auth,
    /// Out of credit or plan limit: stop and surface it, try the backup.
    Quota,
    /// Connection lost or unreachable: retry with backoff, then the backup.
    Network,
    /// The provider answered something we can't use.
    Protocol,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("{kind:?}: {message}")]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    /// Whether the job should move to its backup provider.
    pub fn falls_back(&self) -> bool {
        matches!(self.kind, ProviderErrorKind::Auth | ProviderErrorKind::Quota | ProviderErrorKind::Network)
    }
}

/// Options for one live session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveOptions {
    /// Words to spell right (glossary + attendees, at most 50 × 20 chars).
    pub keyterms: Vec<String>,
    /// BCP-47 language, `"en"` by default.
    pub language: Option<String>,
}

/// What a live provider hears: the current partial (replacing the previous
/// one) or a final. Times are on the side's own audio timeline, in ms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveEvent {
    pub text: String,
    pub is_final: bool,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// How a live session is doing, besides what it hears (the overlay's
/// "Reconnecting").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveStatus {
    /// Captions come from this provider from now on (a job's backup took
    /// over).
    Using(String),
    /// The connection dropped; the provider is reconnecting, and what it
    /// missed is caught up afterwards.
    Reconnecting,
    /// Connected again after `Reconnecting`.
    Connected,
}

/// Live captions for one side.
pub trait LiveStt: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    /// Transcribe `source` live: read `audio` until it closes, send what is
    /// heard to `events`, and return once everything heard has been sent
    /// (what is still partial when the audio closes is sent as final). An
    /// error ends the session; the engine reports it and recording goes on.
    fn run(
        &self,
        source: Source,
        options: LiveOptions,
        audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
    ) -> BoxFuture<'static, Result<(), ProviderError>>;

    /// [`LiveStt::run`], also telling `status` how the connection is doing.
    /// A provider that reconnects says so; by default nothing is told.
    fn run_reporting(
        &self,
        source: Source,
        options: LiveOptions,
        audio: mpsc::UnboundedReceiver<AudioFrame>,
        events: mpsc::UnboundedSender<LiveEvent>,
        status: mpsc::UnboundedSender<LiveStatus>,
    ) -> BoxFuture<'static, Result<(), ProviderError>> {
        drop(status);
        self.run(source, options, audio, events)
    }
}

/// A recorded side to transcribe after Stop.
#[derive(Clone, Debug, Default)]
pub struct BatchRequest {
    /// 16 kHz mono.
    pub samples: Vec<i16>,
    pub diarize: bool,
    pub keyterms: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The diarized speaker (`S1`…), when asked for.
    pub speaker: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchTranscript {
    pub words: Vec<Word>,
}

/// After-Stop transcription of one side.
pub trait BatchStt: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    fn transcribe(&self, request: BatchRequest) -> BoxFuture<'static, Result<BatchTranscript, ProviderError>>;
}

/// An AI provider: a prompt in, JSON matching `schema` out.
pub trait Llm: Send + Sync {
    fn name(&self) -> &str;
    fn complete_json(
        &self,
        prompt: String,
        schema: serde_json::Value,
    ) -> BoxFuture<'static, Result<serde_json::Value, ProviderError>>;
}
