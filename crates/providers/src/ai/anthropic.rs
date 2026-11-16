//! Anthropic's Messages API, native (`anthropic`): key + model, JSON held
//! to the schema with structured outputs.
//!
//! Ticket #10 replaces [`build`] with the real client.

use std::sync::Arc;

use recap_engine::Capabilities;

use crate::{
    provider::{NotBuilt, Provider, ProviderSettings},
    registry::{Family, Kind},
};

pub const KIND: Kind = Kind {
    id: "anthropic",
    family: Family::Ai,
    capabilities: Capabilities { live: false, batch: false, diarize: false, keyterms: false, word_timestamps: false },
    build,
};

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(NotBuilt { name: settings.name.clone(), ticket: "#10" })
}
