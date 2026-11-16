//! Recap's desktop app: the zgx host (Rust) behind a Luau UI (`ui/`).
//!
//! - [`presenter`]: engine events in, the UI's shared state out (pure).
//! - [`host`]: the script host: shared keys, actions, functions.
//! - [`windows`]: each `ui.window`'s native options, opened as its state says.
//! - [`fonts`]: the bundled faces.
//! - [`live`]: how Start begins a meeting (audio, live provider, dev flags).
//! - [`session`]: a live meeting saved as it goes (engine + store).
//! - [`processing`]: the after-Stop pipeline, a resumable job queue of steps.
//! - [`home`]: the meetings list and detail pane, from the store (pure).
//! - [`seed`]: `--seed` stores the test meeting, so Home has one.
//! - [`settings`]: Settings' host side: the providers, their jobs and Test.
//! - [`user_settings`]: the other Settings pages: `Settings` (recap-storage)
//!   saved, published and watched; storage folder and audio retention applied.

pub mod fonts;
pub mod home;
pub mod host;
pub mod live;
pub mod presenter;
pub mod processing;
pub mod seed;
pub mod session;
pub mod settings;
pub mod user_settings;
pub mod windows;
