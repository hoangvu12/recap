//! AI providers: config-driven, three kinds behind `Llm` (text in, JSON
//! out), DESIGN §4.4b. Each kind is one file; the presets users add them
//! from are data in [`presets`].
//!
//! - [`cli`]: a command template (`{prompt}`, `{schema}`,
//!   `{transcript_file}`), transcript on stdin or in a file.
//! - [`openai_compatible`]: base URL + key + model.
//! - [`anthropic`]: the native Messages API.
//!
//! All three share the JSON discipline in [`json`]: the schema goes with
//! the task, the reply is validated, and a reply that doesn't fit is sent
//! back once with the error (ADR 0006).

pub mod anthropic;
pub mod cli;
mod http;
pub mod json;
pub mod openai_compatible;
pub mod presets;

use crate::registry::Kind;

pub const KINDS: [Kind; 3] = [cli::KIND, openai_compatible::KIND, anthropic::KIND];
