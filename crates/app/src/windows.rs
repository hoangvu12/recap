//! The app's windows. Luau declares each one (`ui/windows.luau`: its id,
//! title, size and content) and opens it through its `window.<id>` state;
//! the native options a script can't set (frameless, always on top, minimum
//! size) live here, by id.
//!
//! The first window zgx makes is the hidden root: Recap lives in the tray
//! (`tray`), so no visible window is the app's main one, and closing the
//! last one leaves it running (`Quit::Resident`).

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use zgx_desktop::{Titlebar, WindowBounds, WindowContext, WindowHandle, WindowLevel, WindowOptions};
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
    // Every window a person works in has Windows' minimize, maximize and
    // close over its top right (`Inset`), its own title strip under them
    // (ui/components/title_bar.luau). ADR 0024.
    match id {
        "notes" => WindowOptions { titlebar: Titlebar::Inset, min_size: Some((360., 480.)), ..base },
        "home" => WindowOptions { titlebar: Titlebar::Inset, min_size: Some((640., 400.)), ..base },
        // Settings (screens/8): its own "‹ › Settings" strip.
        "settings" => WindowOptions { titlebar: Titlebar::Inset, min_size: Some((680., 480.)), ..base },
        // The Name speakers dialog (screens/6): its header under the buttons.
        "speakers" => WindowOptions { titlebar: Titlebar::Inset, min_size: Some((420., 360.)), ..base },
        // Captions over the call: frameless and above every window. Opened
        // hidden: `overlay::OverlayHost::attach` makes it a tool window
        // hidden from capture, then shows it. No caption buttons: a tool
        // window has no taskbar button, so a minimized one would be lost
        // (ADR 0020); its own ✕ hides it (ADR 0024).
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

/// A rectangle in physical desktop pixels (a display's work area).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The overlay's bottom edge above the work area's (logical): clear of a
/// call's own controls, over its video (screens/14, 15).
const OVERLAY_ABOVE: f64 = 160.;
/// Notes' gap to the work area's edges (logical).
const NOTES_GAP: f64 = 16.;

/// Where window `id` (`width` × `height`, logical) first opens on work
/// area `area` at `scale`, before it has a saved frame (a saved one wins):
/// the overlay low in the middle, Notes down the right edge, kept within
/// the area's height. Others: where Windows puts them (`None`).
pub fn first_bounds(id: &str, width: f64, height: f64, area: Area, scale: f64) -> Option<WindowBounds> {
    let px = |logical: f64| (logical * scale).round() as i32;
    let (w, h) = (px(width).min(area.w), px(height));
    match id {
        "overlay" => {
            let h = h.min(area.h);
            let y = (area.y + area.h - h - px(OVERLAY_ABOVE)).max(area.y);
            Some(WindowBounds::new(w as u32, h as u32).at(area.x + (area.w - w) / 2, y))
        }
        "notes" => {
            let h = h.min(area.h - 2 * px(NOTES_GAP)).max(1);
            let x = area.x + area.w - w - px(NOTES_GAP);
            Some(WindowBounds::new(w as u32, h as u32).at(x.max(area.x), area.y + px(NOTES_GAP)))
        }
        _ => None,
    }
}

/// The primary display's work area and scale, if Windows tells.
#[cfg(windows)]
pub fn work_area() -> Option<(Area, f64)> {
    use windows::Win32::{
        Foundation::RECT,
        UI::{
            HiDpi::GetDpiForSystem,
            WindowsAndMessaging::{SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW},
        },
    };
    let mut rect = RECT::default();
    // SAFETY: SPI_GETWORKAREA writes one RECT into the pointer given.
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(std::ptr::from_mut(&mut rect).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    // SAFETY: no arguments; the process is DPI aware (winit makes it so).
    let dpi = unsafe { GetDpiForSystem() };
    let area = Area { x: rect.left, y: rect.top, w: rect.right - rect.left, h: rect.bottom - rect.top };
    (area.w > 0 && area.h > 0 && dpi > 0).then_some((area, f64::from(dpi) / 96.))
}

#[cfg(not(windows))]
pub fn work_area() -> Option<(Area, f64)> {
    None
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
                let mut options =
                    options(&window.id, &window.title, window.width, window.height, window.frame_autosave.clone());
                // Seen in the real app: the overlay opened in the middle of
                // the screen, over Home and Notes.
                if let Some((area, scale)) = work_area() {
                    options.bounds = first_bounds(&window.id, window.width, window.height, area, scale);
                }
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
    fn notes_has_the_window_buttons_and_stays_readable() {
        let o = options("notes", "Recap", 420., 900., None);
        assert_eq!(o.titlebar, Titlebar::Inset);
        assert_eq!(o.level, WindowLevel::Normal);
        assert_eq!(o.min_size, Some((360., 480.)));
    }

    #[test]
    fn home_has_the_window_buttons_and_room_for_list_and_detail() {
        let o = options("home", "Recap", 860., 600., Some("home".into()));
        assert_eq!(o.titlebar, Titlebar::Inset);
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
    fn name_speakers_has_the_window_buttons() {
        let o = options("speakers", "Name speakers", 480., 620., Some("speakers".into()));
        assert_eq!(o.titlebar, Titlebar::Inset);
        assert_eq!(o.level, WindowLevel::Normal);
        assert_eq!(o.min_size, Some((420., 360.)));
    }

    #[test]
    fn a_window_without_options_of_its_own_is_a_plain_window() {
        let o = options("dialog", "Dialog", 640., 480., None);
        assert_eq!(o.titlebar, Titlebar::Standard);
        assert_eq!(o.level, WindowLevel::Normal);
    }

    // The laptop's: 1707 × 912 logical at 150%, the taskbar below.
    const LAPTOP: Area = Area { x: 0, y: 0, w: 2560, h: 1368 };

    #[test]
    fn the_overlay_first_opens_low_in_the_middle_not_over_the_screen_centre() {
        let b = first_bounds("overlay", 720., 120., LAPTOP, 1.5).unwrap();
        assert_eq!(b.size, (1080, 180));
        let (x, y) = b.position.unwrap();
        assert_eq!(x, (2560 - 1080) / 2);
        assert_eq!(y, 1368 - 180 - 240);
    }

    #[test]
    fn notes_first_opens_down_the_right_edge_within_the_work_area() {
        let b = first_bounds("notes", 420., 900., LAPTOP, 1.5).unwrap();
        // 900 logical doesn't fit 912 less the gaps: it shrinks to fit.
        assert_eq!(b.size, (630, 1368 - 48));
        assert_eq!(b.position, Some((2560 - 630 - 24, 24)));
        // On a big display it keeps its height.
        let big = Area { x: -1920, y: 0, w: 1920, h: 1160 };
        let b = first_bounds("notes", 420., 900., big, 1.).unwrap();
        assert_eq!(b.size, (420, 900));
        assert_eq!(b.position, Some((-1920 + 1920 - 420 - 16, 16)));
    }

    #[test]
    fn other_windows_open_where_windows_puts_them() {
        assert_eq!(first_bounds("home", 860., 600., LAPTOP, 1.5), None);
        assert_eq!(first_bounds("settings", 800., 732., LAPTOP, 1.5), None);
    }

    #[test]
    fn the_root_window_is_never_seen() {
        assert!(!root().visible);
    }
}
