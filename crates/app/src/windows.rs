//! The app's windows. Luau declares each one (`ui/windows.luau`: its id,
//! title, size and content) and opens it through its `window.<id>` state;
//! the native options a script can't set (frameless, always on top, minimum
//! size) live here, by id.
//!
//! The first window zgx makes is the hidden root: Recap lives in the tray
//! (`tray`), so no visible window is the app's main one, and closing the
//! last one leaves it running (`Quit::Resident`).

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use zgx_desktop::{Titlebar, WindowContext, WindowHandle, WindowLevel, WindowOptions};
use zgx_script::{ScriptHost, Value};

/// The hidden root window: it hosts the script and draws nothing.
pub fn root() -> WindowOptions {
    WindowOptions {
        title: "Recap".into(),
        width: 1.,
        height: 1.,
        visible: false,
        decorations: false,
        ..Default::default()
    }
}

/// The native options of window `id`, around what Luau declared.
pub fn options(id: &str, title: &str, width: f64, height: f64, frame_autosave: Option<String>) -> WindowOptions {
    let base = WindowOptions { title: title.into(), width, height, frame_autosave, ..Default::default() };
    match id {
        // A Raycast-like panel: no title bar (its header moves it).
        "notes" => WindowOptions { titlebar: Titlebar::Hidden, min_size: Some((360., 480.)), ..base },
        // The meetings list: a Raycast window, its search header moves it.
        "home" => WindowOptions { titlebar: Titlebar::Hidden, min_size: Some((640., 400.)), ..base },
        // Settings (screens/8): its own "‹ › Settings" strip, Windows'
        // caption buttons over its top right.
        "settings" => WindowOptions { titlebar: Titlebar::Inset, min_size: Some((680., 480.)), ..base },
        // The Name speakers dialog (screens/6): frameless, its header moves it.
        "speakers" => WindowOptions { titlebar: Titlebar::Hidden, min_size: Some((420., 360.)), ..base },
        // Captions over the call: frameless and above every window. Opened
        // hidden: `overlay::OverlayHost::attach` makes it a tool window
        // hidden from capture, then shows it.
        "overlay" => WindowOptions {
            titlebar: Titlebar::Hidden,
            level: WindowLevel::AlwaysOnTop,
            min_size: Some((360., 80.)),
            visible: false,
            ..base
        },
        _ => base,
    }
}

/// When the app quits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quit {
    /// Once the last declared window closes.
    WhenNoneOpen,
    /// Never on its own (the tray keeps it).
    Resident,
}

/// Called as a window opens: its id, title and context.
pub type Opened = Rc<dyn Fn(&str, &str, &WindowContext)>;

/// The windows [`follow_into`] has open, by id.
#[derive(Clone, Default)]
pub struct OpenWindows(Rc<RefCell<HashMap<String, WindowHandle>>>);

impl OpenWindows {
    /// Bring window `id` forward if it is open (a window opening now comes
    /// forward by itself).
    pub fn focus(&self, id: &str) {
        if let Some(handle) = self.0.borrow().get(id).filter(|h| !h.is_closed()) {
            handle.set_minimized(false);
            handle.request_focus();
        }
    }
}

/// Open and close the script's windows as their `open` states say, and set
/// a state false when its window is closed. Keep the effect it returns.
pub fn follow(host: &ScriptHost, cx: &WindowContext, quit: Quit, devtools: bool) -> zgx::reactive::Effect {
    follow_into(host, cx, quit, devtools, OpenWindows::default(), Rc::new(|_, _, _| {}))
}

/// [`follow`], keeping the open windows in `open` (to bring one forward);
/// `opened` hears each window that opens, once its native window exists.
pub fn follow_into(
    host: &ScriptHost,
    cx: &WindowContext,
    quit: Quit,
    devtools: bool,
    open: OpenWindows,
    opened: Opened,
) -> zgx::reactive::Effect {
    let factory = cx.windows.clone();
    let runtime = cx.ui.runtime.clone();
    let open = open.0;
    let ever_open = Rc::new(std::cell::Cell::new(false));
    let (list, host) = (host.windows(), host.clone());
    cx.ui.runtime.effect(move || {
        let windows = list.get();
        let mut any_wanted = false;
        for window in windows {
            let wanted = window.open.get().truthy();
            any_wanted |= wanted;
            let shown = open.borrow().get(&window.id).is_some_and(|handle| !handle.is_closed());
            if wanted && !shown {
                let options =
                    options(&window.id, &window.title, window.width, window.height, window.frame_autosave.clone());
                let (view_host, state, id, title) =
                    (host.clone(), window.open.clone(), window.id.clone(), window.title.clone());
                let opened = opened.clone();
                let handle = factory.open(options, move |wcx| {
                    opened(&id, &title, wcx);
                    let closing = id.clone();
                    wcx.on_closed(move || {
                        if state.with_untracked(Value::truthy) {
                            state.set(Value::Bool(false));
                        }
                        zgx_script::inspect::forget_window(&closing);
                    });
                    let handle = wcx.render(view_host.window_view(&id));
                    if devtools {
                        let session =
                            zgx_script::inspect::Session::new(&wcx.ui, handle, &view_host, Some(wcx.inspector.clone()));
                        zgx_script::inspect::register_window(&id, &title, session);
                    }
                });
                open.borrow_mut().insert(window.id.clone(), handle);
            } else if !wanted
                && shown
                && let Some(handle) = open.borrow_mut().remove(&window.id)
            {
                runtime.untracked(|| handle.close());
            }
        }
        ever_open.set(ever_open.get() || any_wanted);
        if quit == Quit::WhenNoneOpen && ever_open.get() && !any_wanted {
            factory.quit();
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_overlay_floats_frameless_over_every_window() {
        let o = options("overlay", "Recap captions", 720., 120., Some("overlay".into()));
        assert_eq!(o.level, WindowLevel::AlwaysOnTop);
        assert_eq!(o.titlebar, Titlebar::Hidden);
        assert_eq!((o.width, o.height), (720., 120.));
        assert_eq!(o.frame_autosave.as_deref(), Some("overlay"));
        // Shown once it is a tool window hidden from capture.
        assert!(!o.visible);
    }

    #[test]
    fn notes_is_a_frameless_panel_that_stays_readable() {
        let o = options("notes", "Recap", 420., 900., None);
        assert_eq!(o.titlebar, Titlebar::Hidden);
        assert_eq!(o.level, WindowLevel::Normal);
        assert_eq!(o.min_size, Some((360., 480.)));
    }

    #[test]
    fn home_is_a_frameless_window_wide_enough_for_list_and_detail() {
        let o = options("home", "Recap", 860., 600., Some("home".into()));
        assert_eq!(o.titlebar, Titlebar::Hidden);
        assert_eq!(o.level, WindowLevel::Normal);
        assert_eq!(o.min_size, Some((640., 400.)));
    }

    #[test]
    fn settings_draws_its_own_title_strip_under_the_caption_buttons() {
        let o = options("settings", "Settings", 800., 732., Some("settings".into()));
        assert_eq!(o.titlebar, Titlebar::Inset);
        assert_eq!(o.min_size, Some((680., 480.)));
    }

    #[test]
    fn name_speakers_is_a_frameless_dialog() {
        let o = options("speakers", "Name speakers", 480., 620., Some("speakers".into()));
        assert_eq!(o.titlebar, Titlebar::Hidden);
        assert_eq!(o.level, WindowLevel::Normal);
        assert_eq!(o.min_size, Some((420., 360.)));
    }

    #[test]
    fn a_window_without_options_of_its_own_is_a_plain_window() {
        let o = options("dialog", "Dialog", 640., 480., None);
        assert_eq!(o.titlebar, Titlebar::Standard);
        assert_eq!(o.level, WindowLevel::Normal);
    }

    #[test]
    fn the_root_window_is_never_seen() {
        assert!(!root().visible);
    }
}
