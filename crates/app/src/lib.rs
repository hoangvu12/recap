//! Recap's desktop app: the zgx host (Rust) behind a Luau UI (`ui/`).
//!
//! - [`presenter`]: engine events in, the UI's shared state out (pure).
//! - [`host`]: the script host: shared keys, actions, functions.
//! - [`windows`]: each `ui.window`'s native options, opened as its state says.
//! - [`fonts`]: the bundled faces.
//! - [`live`]: how Start begins a meeting (audio, live provider, dev flags).
//! - [`session`]: a live meeting saved as it goes (engine + store).
//! - [`home`]: the meetings list and detail pane, from the store (pure).
//! - [`seed`]: `--seed` stores the test meeting, so Home has one.
//! - [`settings`]: Settings' host side: the providers, their jobs and Test.
//! - [`user_settings`]: the other Settings pages: `Settings` (recap-storage)
//!   saved, published and watched; storage folder and audio retention applied.
//! - [`commands`]: what the tray, the hotkeys and the call toasts do.
//! - [`tray`], [`hotkeys`], [`calls`], [`autostart`]: Recap living in the
//!   tray (ticket #15, ADR 0011).

pub mod autostart;
pub mod calls;
pub mod commands;
pub mod fonts;
pub mod home;
pub mod host;
pub mod hotkeys;
pub mod live;
pub mod presenter;
pub mod seed;
pub mod session;
pub mod settings;
pub mod tray;
pub mod user_settings;
pub mod windows;
