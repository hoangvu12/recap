//! What a provider kind builds: the engines it offers and its Test.

use std::sync::Arc;

use recap_engine::{BatchStt, BoxFuture, LiveStt, Llm, ProviderError, ProviderErrorKind};

use crate::config::TranscriptInput;

/// One provider's settings, its key included (read from the secret store,
/// or typed in a form not saved yet). What a [`crate::Kind`] builds from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderSettings {
    /// Shown to the user, and the engines' `name()`.
    pub name: String,
    pub key: Option<String>,
    pub model: String,
    pub base_url: String,
    /// A `cli` provider's command template (`{prompt}`, `{schema}`,
    /// `{transcript_file}`).
    pub command: String,
    /// How a `cli` provider gets the transcript.
    pub input: TranscriptInput,
}

/// A built provider: the engines it offers for jobs, and a quick check that
/// it works (the Test button).
pub trait Provider: Send + Sync {
    /// Live captions (`Capabilities::live`).
    fn live(&self) -> Option<Arc<dyn LiveStt>> {
        None
    }
    /// The after-Stop transcript (`Capabilities::batch`).
    fn batch(&self) -> Option<Arc<dyn BatchStt>> {
        None
    }
    /// Summary and names (AI providers).
    fn llm(&self) -> Option<Arc<dyn Llm>> {
        None
    }
    /// The smallest real request that proves the key, model or command
    /// works. Its time is the latency the Test button shows.
    fn test(&self) -> BoxFuture<'static, Result<(), ProviderError>>;
    /// Why it offers no engine for `job` (a job label).
    fn not_offered(&self, job: &str) -> ProviderError {
        ProviderError::new(ProviderErrorKind::Other, format!("this provider can't do {job}"))
    }
}

/// The stand-in for an adapter whose ticket hasn't landed: it offers no
/// engine, and its Test says so.
pub struct NotBuilt {
    pub name: String,
    /// The ticket that builds it ("#6").
    pub ticket: &'static str,
}

impl NotBuilt {
    pub fn error(&self) -> ProviderError {
        ProviderError::new(ProviderErrorKind::Other, format!("{} is not built yet ({})", self.name, self.ticket))
    }
}

impl Provider for NotBuilt {
    fn test(&self) -> BoxFuture<'static, Result<(), ProviderError>> {
        let error = self.error();
        Box::pin(async move { Err(error) })
    }
    fn not_offered(&self, _job: &str) -> ProviderError {
        self.error()
    }
}
