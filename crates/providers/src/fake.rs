//! Fake provider kinds for tests (not in [`crate::Registry::builtin`]):
//! add them with `Registry::builtin().with(fake::AI, [fake::ai_preset()])`.
//!
//! A fake answers by its model: `""` works; `"quota"`, `"auth"`,
//! `"network"` or `"protocol"` fail so (its Test and its engine alike).

use std::sync::Arc;

use recap_engine::{BoxFuture, Capabilities, Llm, ProviderError, ProviderErrorKind};

use crate::{
    provider::{NotBuilt, Provider, ProviderSettings},
    registry::{Family, Fields, Group, Kind, Preset},
};

/// An AI provider.
pub const AI: Kind = Kind { id: "fake_ai", family: Family::Ai, capabilities: NONE, build: build_ai };

pub fn ai_preset() -> Preset {
    Preset {
        fields: Fields { key: false, model: true, base_url: false, command: false, name: false },
        ..Preset::new("fake_ai", "Fake AI", "compatible", Group::Api, "fake_ai")
    }
}

/// A speech engine that can only transcribe after Stop, without speakers:
/// no job but none can take it.
pub const BATCH_ONLY: Kind = Kind {
    id: "batch_only",
    family: Family::Stt,
    capabilities: Capabilities { batch: true, word_timestamps: true, ..NONE },
    build: build_stt,
};

pub fn batch_only_preset() -> Preset {
    Preset::new("batch_only", "Batch only", "command", Group::Speech, "batch_only")
}

const NONE: Capabilities =
    Capabilities { live: false, batch: false, diarize: false, keyterms: false, word_timestamps: false };

fn outcome(model: &str) -> Result<(), ProviderError> {
    let kind = match model {
        "quota" => ProviderErrorKind::Quota,
        "auth" => ProviderErrorKind::Auth,
        "network" => ProviderErrorKind::Network,
        "protocol" => ProviderErrorKind::Protocol,
        _ => return Ok(()),
    };
    let message = match kind {
        ProviderErrorKind::Quota => "Out of credit (fake)",
        ProviderErrorKind::Auth => "Key refused (fake)",
        ProviderErrorKind::Network => "Can't reach it (fake)",
        _ => "Unexpected answer (fake)",
    };
    Err(ProviderError::new(kind, message))
}

struct FakeAi(ProviderSettings);

impl Provider for FakeAi {
    fn llm(&self) -> Option<Arc<dyn Llm>> {
        Some(Arc::new(FakeLlm(self.0.clone())))
    }
    fn test(&self) -> BoxFuture<'static, Result<(), ProviderError>> {
        let result = outcome(&self.0.model);
        Box::pin(async move { result })
    }
}

struct FakeLlm(ProviderSettings);

impl Llm for FakeLlm {
    fn name(&self) -> &str {
        &self.0.name
    }
    fn complete_json(
        &self,
        _prompt: String,
        _schema: serde_json::Value,
    ) -> BoxFuture<'static, Result<serde_json::Value, ProviderError>> {
        let answer = outcome(&self.0.model).map(|()| serde_json::json!({ "provider": self.0.name }));
        Box::pin(async move { answer })
    }
}

fn build_ai(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(FakeAi(settings.clone()))
}

fn build_stt(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(NotBuilt { name: settings.name.clone(), ticket: "a fake" })
}
