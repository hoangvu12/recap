//! The registry: provider kinds (code) and the presets users add providers
//! from (data).

use std::sync::Arc;

use recap_engine::Capabilities;
use serde::Serialize;

use crate::{
    ai,
    provider::{Provider, ProviderSettings},
    stt,
};

/// Speech-to-text or AI: which jobs a kind can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Stt,
    Ai,
}

/// Where a preset shows in the Add provider list (screens/8c), and the
/// speech engines (not added: they are code).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    /// Speech-to-text, always listed.
    Speech,
    /// "API": a key and a model.
    Api,
    /// "On this PC": a local server.
    Local,
    /// "Command line": an installed CLI.
    Cli,
    /// "Custom": any OpenAI-compatible endpoint or command.
    Custom,
}

/// What a provider's form asks for (screens/8d, 8e).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Fields {
    /// An API key (kept in Credential Manager).
    pub key: bool,
    pub model: bool,
    pub base_url: bool,
    /// A command template and how the transcript goes in.
    pub command: bool,
    /// The user names it (custom entries).
    pub name: bool,
}

/// A provider kind: code that builds providers from their settings.
#[derive(Clone, Copy)]
pub struct Kind {
    pub id: &'static str,
    pub family: Family,
    /// Speech kinds: what they can do. AI kinds: none.
    pub capabilities: Capabilities,
    pub build: fn(&ProviderSettings) -> Arc<dyn Provider>,
}

impl std::fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kind").field("id", &self.id).field("family", &self.family).finish_non_exhaustive()
    }
}

/// A named starting point for a provider of some kind: its logo, group and
/// defaults. Speech engines are one preset each, with their kind's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    /// A logo of `ui/lib/logos.luau` (the Provider logos board).
    pub logo: &'static str,
    pub group: Group,
    /// Its [`Kind`]'s id.
    pub kind: &'static str,
    pub fields: Fields,
    pub base_url: &'static str,
    pub model: &'static str,
    pub command: &'static str,
    /// Shown at the end of its row in the Add provider list: a command
    /// (`claude`) or where it runs (`localhost:11434`).
    pub detail: &'static str,
    /// The key field's placeholder (`sk-or-…`).
    pub key_hint: &'static str,
}

impl Preset {
    /// A preset with no defaults.
    pub const fn new(
        id: &'static str,
        name: &'static str,
        logo: &'static str,
        group: Group,
        kind: &'static str,
    ) -> Self {
        Self {
            id,
            name,
            logo,
            group,
            kind,
            fields: Fields { key: false, model: false, base_url: false, command: false, name: false },
            base_url: "",
            model: "",
            command: "",
            detail: "",
            key_hint: "",
        }
    }
}

/// Every kind and preset the app knows.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    kinds: Vec<Kind>,
    presets: Vec<Preset>,
}

impl Registry {
    /// What ships: the speech engines, then the AI kinds and their presets.
    pub fn builtin() -> Self {
        let mut registry = Self::default();
        for (kind, preset) in stt::ALL {
            registry = registry.with(kind, [preset]);
        }
        for kind in ai::KINDS {
            registry = registry.with(kind, []);
        }
        registry.presets.extend(ai::presets::ALL);
        registry
    }

    /// This registry and `kind` with its `presets` (tests add fakes so).
    pub fn with(mut self, kind: Kind, presets: impl IntoIterator<Item = Preset>) -> Self {
        self.kinds.retain(|k| k.id != kind.id);
        self.kinds.push(kind);
        self.presets.extend(presets);
        self
    }

    pub fn kind(&self, id: &str) -> Option<&Kind> {
        self.kinds.iter().find(|k| k.id == id)
    }

    pub fn preset(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// In the order the Add provider list shows them.
    pub fn presets(&self) -> &[Preset] {
        &self.presets
    }

    pub fn kinds(&self) -> &[Kind] {
        &self.kinds
    }
}
