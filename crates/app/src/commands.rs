//! What the tray menu, the global hotkeys and the call toasts can do: one
//! set of commands on the host, so each does the same thing wherever it is
//! asked from.

use std::rc::Rc;

use crate::host::Host;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Start notes, or stop the live ones (the Start notes hotkey, the
    /// tray's first item).
    ToggleNotes,
    /// Pause the live meeting, or resume it.
    PauseResume,
    /// Show or hide the caption overlay.
    ToggleCaptions,
    /// The overlay lets clicks through, or stops.
    ToggleClickThrough,
    OpenHome,
    OpenSettings,
    /// Quit Recap (a live meeting is stopped and saved first).
    Quit,
}

impl Command {
    /// The command of Settings' hotkey `id` (`recap_storage::Hotkeys`).
    pub fn for_hotkey(id: &str) -> Option<Command> {
        Some(match id {
            "start_notes" => Command::ToggleNotes,
            "pause" => Command::PauseResume,
            "click_through" => Command::ToggleClickThrough,
            "captions" => Command::ToggleCaptions,
            _ => return None,
        })
    }
}

/// Runs commands on the host. What it can't do itself it is given: how to
/// start notes (main.rs's Start), bring a window forward, and quit.
#[derive(Clone)]
pub struct Commands {
    host: Host,
    start: Rc<dyn Fn()>,
    focus: Rc<dyn Fn(&str)>,
    quit: Rc<dyn Fn()>,
}

impl Commands {
    pub fn new(host: Host, start: Rc<dyn Fn()>, focus: Rc<dyn Fn(&str)>, quit: Rc<dyn Fn()>) -> Self {
        Self { host, start, focus, quit }
    }

    pub fn run(&self, command: Command) {
        let host = &self.host;
        match command {
            Command::ToggleNotes => {
                if host.is_live() {
                    host.stop();
                } else {
                    (self.start)();
                }
            }
            Command::PauseResume => host.toggle_pause(),
            Command::ToggleCaptions => host.toggle_window("overlay"),
            Command::ToggleClickThrough => host.toggle_click_through(),
            Command::OpenHome => self.show("home"),
            Command::OpenSettings => self.show("settings"),
            Command::Quit => {
                host.stop();
                let quit = self.quit.clone();
                host.when_idle(move || quit());
            }
        }
    }

    fn show(&self, id: &str) {
        self.host.open_window(id);
        (self.focus)(id);
    }
}
