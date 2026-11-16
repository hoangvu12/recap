//! Recap's desktop app: the zgx host (Rust) behind a Luau UI (`ui/`).
//!
//! - [`presenter`]: engine events in, the UI's shared state out (pure).
//! - [`host`]: the script host: shared keys, actions, functions.
//! - [`windows`]: each `ui.window`'s native options, opened as its state says.
//! - [`fake`]: a meeting replayed from the test script, for the UI without audio.
//! - [`fonts`]: the bundled faces.

pub mod fake;
pub mod fonts;
pub mod host;
pub mod presenter;
pub mod windows;
