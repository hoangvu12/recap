//! Deepgram Nova-3, live and pre-recorded: the backup speech engine
//! (DESIGN §4.2). Its adapter replaces [`build`].

use std::sync::Arc;

use recap_engine::Capabilities;

use crate::{
    provider::{NotBuilt, Provider, ProviderSettings},
    registry::{Family, Fields, Group, Kind, Preset},
};

pub const KIND: Kind = Kind {
    id: "deepgram",
    family: Family::Stt,
    capabilities: Capabilities { live: true, batch: true, diarize: true, keyterms: true, word_timestamps: true },
    build,
};

pub const PRESET: Preset = Preset {
    fields: Fields { key: true, model: false, base_url: false, command: false, name: false },
    ..Preset::new("deepgram", "Deepgram", "deepgram", Group::Speech, "deepgram")
};

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(NotBuilt { name: settings.name.clone(), ticket: "Deepgram adapter" })
}
