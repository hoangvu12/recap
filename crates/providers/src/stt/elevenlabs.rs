//! ElevenLabs Scribe: live (`scribe_v3_live`) and batch (`scribe_v3`).
//! DESIGN §4.2, §4.3; the tested parameters are in `reference/`.
//!
//! The adapter itself is ticket #6 (live) and the processing ticket
//! (batch): replace [`build`] with one that returns the real engines.

use std::sync::Arc;

use recap_engine::Capabilities;

use crate::{
    provider::{NotBuilt, Provider, ProviderSettings},
    registry::{Family, Fields, Group, Kind, Preset},
};

pub const KIND: Kind = Kind {
    id: "elevenlabs",
    family: Family::Stt,
    capabilities: Capabilities { live: true, batch: true, diarize: true, keyterms: true, word_timestamps: true },
    build,
};

pub const PRESET: Preset = Preset {
    fields: Fields { key: true, model: false, base_url: false, command: false, name: false },
    key_hint: "sk_…",
    ..Preset::new("elevenlabs", "ElevenLabs", "elevenlabs", Group::Speech, "elevenlabs")
};

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(NotBuilt { name: settings.name.clone(), ticket: "#6" })
}
