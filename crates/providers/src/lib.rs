//! Recap's providers (GLOSSARY: Provider, Job, Capability; ADR 0005).
//!
//! - [`Registry`]: the provider **kinds** (code: an adapter's builder and,
//!   for speech, its capabilities) and the **presets** the user adds
//!   providers from (data: name, logo, group, defaults). [`Registry::builtin`]
//!   lists them all: `stt/<vendor>.rs` (one file per speech engine) and
//!   `ai/` (the three AI kinds plus `ai/presets.rs`).
//! - [`Provider`]: what a kind builds from a provider's [`ProviderSettings`]:
//!   live and/or batch STT, or an `Llm`, and a `test()`.
//! - [`Providers`]: the user's providers and, for each [`Job`], a main and a
//!   backup; saved to `providers.json` (no keys), keys in [`Secrets`]
//!   (Credential Manager). It builds each job as one provider with
//!   fallback ([`recap_engine::Fallback`]) and runs the Test button.
//!
//! Adding a speech engine: one `stt/<vendor>.rs` with its `KIND` and
//! `PRESET`, and a line in [`Registry::builtin`]. Adding an AI preset: one
//! entry in `ai/presets.rs`. No UI work: the Providers page draws what
//! [`Providers::view`] says.

mod ai;
mod config;
pub mod fake;
mod job;
mod provider;
mod registry;
mod secrets;
mod service;
mod stt;

pub use config::TranscriptInput;
pub use job::{Job, Slot};
pub use provider::{NotBuilt, Provider, ProviderSettings};
pub use registry::{Family, Fields, Group, Kind, Preset, Registry};
pub use secrets::{Keyring, MemorySecrets, SecretError, Secrets, mask};
pub use service::{Draft, Providers, ProvidersError, TestOutcome};
