//! Any OpenAI-compatible chat API (`openai_compatible`): base URL + key +
//! model. OpenAI, OpenRouter, Groq, DeepSeek, Mistral, Gemini and Ollama
//! are presets of it. Replies that can't be held to the schema are
//! validated and retried once with the error (DESIGN §4.4b).
//!
//! Ticket #10 replaces [`build`] with the real client.

use std::sync::Arc;

use recap_engine::Capabilities;

use crate::{
    provider::{NotBuilt, Provider, ProviderSettings},
    registry::{Family, Kind},
};

pub const KIND: Kind = Kind {
    id: "openai_compatible",
    family: Family::Ai,
    capabilities: Capabilities { live: false, batch: false, diarize: false, keyterms: false, word_timestamps: false },
    build,
};

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(NotBuilt { name: settings.name.clone(), ticket: "#10" })
}
