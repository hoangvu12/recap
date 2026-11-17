//! Recap's desktop app: the zgx host (Rust) behind a Luau UI (`ui/`).
//!
//! - [`presenter`]: engine events in, the UI's shared state out (pure).
//! - [`host`]: the script host: shared keys, actions, functions.
//! - [`windows`]: each `ui.window`'s native options, opened as its state says.
//! - [`fonts`]: the bundled faces.
//! - [`live`]: how Start begins a meeting (audio, live provider, dev flags).
//! - [`live_meeting`]: a live meeting saved as it goes (engine + store).
//! - [`processing`]: the after-Stop pipeline, a resumable job queue of steps.
//! - [`naming`], [`summary`]: its AI steps, Naming speakers and Writing
//!   summary; the summary's Markdown, its ticks and the Summary tab's state.
//! - [`home`]: the meetings list, search and detail pane, from the store (pure).
//! - [`export`]: a meeting as Markdown, its export file name, Show in folder.
//! - [`seed`]: `--seed` stores the test meeting, so Home has one.
//! - [`settings`]: Settings' host side: the providers, their jobs and Test.
//! - [`user_settings`]: the other Settings pages: `Settings` (recap-storage)
//!   saved, published and watched; storage folder and audio retention applied.
//! - [`overlay`]: the caption overlay's window: hidden from capture, a tool
//!   window, click-through and pin; switching the live engine.
//! - [`speakers`]: naming speakers by hand: the Name speakers dialog's state
//!   and renames kept.
//! - [`playback`]: a line or a voice of the recording, played on a thread
//!   of its own (rodio).
//! - [`commands`]: what the tray, the hotkeys and the call toasts do.
//! - [`tray`], [`hotkeys`], [`calls`], [`autostart`]: Recap living in the
//!   tray (ticket #15, ADR 0011).
//! - [`soak`]: `--soak <minutes>`, the long-run leak check (ticket #18).

pub mod autostart;
pub mod calls;
pub mod commands;
pub mod export;
pub mod fonts;
pub mod home;
pub mod host;
pub mod hotkeys;
pub mod live;
pub mod live_meeting;
pub mod naming;
pub mod overlay;
pub mod playback;
pub mod presenter;
pub mod processing;
pub mod seed;
pub mod settings;
pub mod soak;
pub mod speakers;
pub mod summary;
pub mod tray;
pub mod user_settings;
pub mod windows;
