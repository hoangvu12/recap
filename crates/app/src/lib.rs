//! Recap's desktop app: the zgx host (Rust) behind a Luau UI (`ui/`).
//!
//! - [`presenter`]: engine events in, the UI's shared state out (pure).
//! - [`host`]: the script host: shared keys, actions, functions.
//! - [`windows`]: each `ui.window`'s native options, opened as its state says.
//! - [`fonts`]: the bundled faces.
//! - [`live`]: how Start begins a meeting (audio, live provider, dev flags).
//! - [`session`]: a live meeting saved as it goes (engine + store).
//! - [`processing`]: the after-Stop pipeline, a resumable job queue of steps.
//! - [`naming`], [`summary`]: its AI steps, Naming speakers and Writing
//!   summary; the summary's Markdown, its ticks and the Summary tab's state.
//! - [`home`]: the meetings list and detail pane, from the store (pure).
//! - [`seed`]: `--seed` stores the test meeting, so Home has one.
//! - [`settings`]: Settings' host side: the providers, their jobs and Test.
//! - [`user_settings`]: the other Settings pages: `Settings` (recap-storage)
//!   saved, published and watched; storage folder and audio retention applied.
//! - [`speakers`]: naming speakers by hand: the Name speakers dialog's state
//!   and renames kept.
//! - [`playback`]: a line or a voice of the recording, played on a thread
//!   of its own (rodio).

pub mod fonts;
pub mod home;
pub mod host;
pub mod live;
pub mod naming;
pub mod playback;
pub mod presenter;
pub mod processing;
pub mod seed;
pub mod session;
pub mod settings;
pub mod speakers;
pub mod summary;
pub mod user_settings;
pub mod windows;
