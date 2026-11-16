//! A command-line AI (`cli`): Claude Code, Codex, Gemini CLI or a custom
//! command. The command template's `{prompt}`, `{schema}` and
//! `{transcript_file}` are filled in; the transcript goes on stdin or in a
//! file (`ProviderSettings::input`). Claude Code runs with
//! `ANTHROPIC_API_KEY` removed from its environment (DESIGN §4.4).
//!
//! Ticket #10 replaces [`build`] with the real runner.

use std::sync::Arc;

use recap_engine::Capabilities;

use crate::{
    provider::{NotBuilt, Provider, ProviderSettings},
    registry::{Family, Kind},
};

pub const KIND: Kind = Kind { id: "cli", family: Family::Ai, capabilities: NONE, build };

const NONE: Capabilities =
    Capabilities { live: false, batch: false, diarize: false, keyterms: false, word_timestamps: false };

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(NotBuilt { name: settings.name.clone(), ticket: "#10" })
}

/// The program a command template runs (`claude` for `claude -p …`).
pub fn program(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or("")
}
