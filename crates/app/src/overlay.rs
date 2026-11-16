//! The caption overlay's window (screens/1): what Luau can't do for it.
//!
//! - **Hidden from screen sharing:** `SetWindowDisplayAffinity(
//!   WDA_EXCLUDEFROMCAPTURE)` while Settings' `hide_from_share` is on,
//!   applied again as it changes.
//! - **A tool window:** no taskbar button. zgx has no option for it and
//!   winit rewrites the extended style on every flag change (show, level,
//!   click-through), so the window is subclassed and `WM_STYLECHANGING`
//!   keeps `WS_EX_TOOLWINDOW` (and drops `WS_EX_APPWINDOW`) each time.
//! - **Click-through** (whole window, zgx's pointer passthrough) and **pin**
//!   (always on top, or among the other windows again).
//!
//! The window opens hidden (`windows::options`); once its HWND is found
//! (the HWND workaround in research_zgx.md: our thread's windows, by title)
//! and styled, it is shown, so it never shows on the taskbar.
//!
//! ```text
//! shared  overlay        { click_through, pinned }
//! action  toggle_click_through       (the toolbar; #15's hotkey calls OverlayHost::toggle_click_through)
//!         set_overlay_pinned(bool)
//!         switch_live({ id, name })  caption with that engine where the live one failed
//! ```
//! It also follows `caption_my_speech` live (the toolbar's "show my lines").

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use recap_providers::Providers;
use serde::Deserialize;
use zgx_desktop::{WindowContext, WindowHandle, WindowLevel};
use zgx_script::{Shared, Value};

use crate::{host::Host, user_settings::SettingsHost};

/// How long to look for the window's HWND before showing it anyway.
const FIND_FOR: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct OverlayHost(Rc<Inner>);

struct Inner {
    state: Shared,
    click_through: Cell<bool>,
    pinned: Cell<bool>,
    hide_from_share: Cell<bool>,
    window: RefCell<Option<WindowHandle>>,
    /// The open overlay's HWND (0: none found, or closed).
    hwnd: Cell<isize>,
}

#[derive(Deserialize)]
struct LiveChoice {
    id: String,
    name: String,
}

impl OverlayHost {
    /// The overlay's state and actions on `host`'s script, following
    /// `settings`; `providers` builds the engine a switch asks for.
    pub fn register(host: &Host, settings: &SettingsHost, providers: Providers) -> Self {
        let overlay = Self(Rc::new(Inner {
            state: host.script.shared("overlay", Value::Nil),
            click_through: Cell::new(false),
            pinned: Cell::new(true),
            hide_from_share: Cell::new(settings.current().hide_from_share),
            window: RefCell::default(),
            hwnd: Cell::new(0),
        }));
        overlay.publish();

        let o = overlay.clone();
        host.script.action("toggle_click_through", move |_| o.toggle_click_through());
        let o = overlay.clone();
        host.script.action("set_overlay_pinned", move |arg| o.set_pinned(arg.truthy()));
        let h = host.clone();
        host.script.action("switch_live", move |arg| {
            let Ok(choice) = serde_json::from_str::<LiveChoice>(&arg.to_json()) else { return };
            match providers.build(&choice.id) {
                Ok(provider) => match provider.live() {
                    Some(live) => h.replace_live(live),
                    None => h.switch_failed(&choice.name, &provider.not_offered("Live captions").message),
                },
                Err(error) => h.switch_failed(&choice.name, &error.message),
            }
        });

        let (o, h) = (overlay.clone(), host.clone());
        settings.on_change(move |old, new| {
            if old.hide_from_share != new.hide_from_share {
                o.0.hide_from_share.set(new.hide_from_share);
                o.apply_capture();
            }
            if old.caption_my_speech != new.caption_my_speech {
                h.set_my_captions(new.caption_my_speech);
            }
        });
        overlay
    }

    /// The overlay window `title` was just opened in `wcx` (hidden): style
    /// it, then show it.
    pub fn attach(&self, title: &str, wcx: &WindowContext) {
        let window = wcx.window.clone();
        *self.0.window.borrow_mut() = Some(window.clone());
        self.0.hwnd.set(0);
        if self.0.click_through.get() {
            window.set_pointer_passthrough(true);
        }
        if !self.0.pinned.get() {
            window.set_level(WindowLevel::Normal);
        }
        let (overlay, title) = (self.clone(), title.to_owned());
        wcx.tasks.spawn(async move {
            let started = std::time::Instant::now();
            let hwnd = loop {
                match win32::find(&title) {
                    Some(hwnd) => break Some(hwnd),
                    None if started.elapsed() > FIND_FOR => break None,
                    None => zgx::timer::sleep(Duration::from_millis(16)).await,
                }
            };
            match hwnd {
                Some(hwnd) => {
                    overlay.0.hwnd.set(hwnd);
                    win32::keep_tool_window(hwnd);
                    overlay.apply_capture();
                    #[cfg(windows)]
                    eprintln!(
                        "recap: overlay {hwnd:#x}: display affinity {:#x}, extended style {:#x}",
                        win32::capture_affinity(hwnd),
                        win32::ex_style(hwnd)
                    );
                }
                None => eprintln!("recap: the overlay's window wasn't found; it shows on the taskbar"),
            }
            window.set_visible(true);
        });
    }

    /// Clicks go through the overlay to the window under it, or no longer.
    pub fn toggle_click_through(&self) {
        self.set_click_through(!self.0.click_through.get());
    }

    pub fn set_click_through(&self, on: bool) {
        self.0.click_through.set(on);
        if let Some(window) = self.open_window() {
            window.set_pointer_passthrough(on);
        }
        self.publish();
    }

    /// Whether click-through is on.
    pub fn click_through(&self) -> bool {
        self.0.click_through.get()
    }

    /// Above every window (pinned), or among them.
    pub fn set_pinned(&self, on: bool) {
        self.0.pinned.set(on);
        if let Some(window) = self.open_window() {
            window.set_level(if on { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal });
        }
        self.publish();
    }

    /// The open overlay's HWND, once found (manual checks, logs).
    pub fn hwnd(&self) -> Option<isize> {
        Some(self.0.hwnd.get()).filter(|h| *h != 0 && self.open_window().is_some())
    }

    fn open_window(&self) -> Option<WindowHandle> {
        self.0.window.borrow().clone().filter(|w| !w.is_closed())
    }

    fn apply_capture(&self) {
        if let Some(hwnd) = self.hwnd() {
            win32::exclude_from_capture(hwnd, self.0.hide_from_share.get());
        }
    }

    fn publish(&self) {
        let json = serde_json::json!({ "click_through": self.0.click_through.get(), "pinned": self.0.pinned.get() });
        self.0.state.set(Value::from_json(&json.to_string()).unwrap_or(Value::Nil));
    }
}

/// The extended style a tool window keeps: no taskbar button.
pub fn tool_window_style(ex_style: u32) -> u32 {
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    const WS_EX_APPWINDOW: u32 = 0x0004_0000;
    (ex_style | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW
}

#[cfg(windows)]
pub mod win32 {
    //! The overlay's raw Win32. HWNDs travel as `isize`.

    use ::windows::{
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            System::Threading::GetCurrentThreadId,
            UI::{
                Shell::{DefSubclassProc, SetWindowSubclass},
                WindowsAndMessaging::{
                    EnumThreadWindows, GWL_EXSTYLE, GetWindowDisplayAffinity, GetWindowLongPtrW, GetWindowTextW,
                    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowDisplayAffinity,
                    SetWindowLongPtrW, SetWindowPos, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WM_STYLECHANGING,
                },
            },
        },
        core::BOOL,
    };

    use super::tool_window_style;

    fn hwnd(raw: isize) -> HWND {
        HWND(raw as *mut core::ffi::c_void)
    }

    /// This thread's window titled `title`.
    pub fn find(title: &str) -> Option<isize> {
        struct Search {
            title: Vec<u16>,
            found: isize,
        }
        unsafe extern "system" fn each(window: HWND, lparam: LPARAM) -> BOOL {
            // SAFETY: lparam is the &mut Search below, alive for the call.
            let search = unsafe { &mut *(lparam.0 as *mut Search) };
            let mut text = [0u16; 256];
            // SAFETY: a window handle the system enumerates.
            let n = unsafe { GetWindowTextW(window, &mut text) }.max(0) as usize;
            if text[..n] == search.title[..] {
                search.found = window.0 as isize;
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut search = Search { title: title.encode_utf16().collect(), found: 0 };
        // SAFETY: the callback reads `search` only during this call.
        unsafe {
            let _ = EnumThreadWindows(GetCurrentThreadId(), Some(each), LPARAM(&mut search as *mut Search as isize));
        }
        (search.found != 0).then_some(search.found)
    }

    #[repr(C)]
    struct StyleStruct {
        old: u32,
        new: u32,
    }

    const SUBCLASS_ID: usize = 0x5265_6361; // "Reca"

    unsafe extern "system" fn keep_tool(
        window: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 {
            // SAFETY: WM_STYLECHANGING's lparam is a STYLESTRUCT to change.
            let style = unsafe { &mut *(lparam.0 as *mut StyleStruct) };
            style.new = tool_window_style(style.new);
        }
        // SAFETY: passing the message on to the window's own procedure.
        unsafe { DefSubclassProc(window, msg, wparam, lparam) }
    }

    /// Make `raw` a tool window (no taskbar button) and keep it one.
    pub fn keep_tool_window(raw: isize) {
        let window = hwnd(raw);
        // SAFETY: a window of this thread, found by `find`.
        unsafe {
            if !SetWindowSubclass(window, Some(keep_tool), SUBCLASS_ID, 0).as_bool() {
                eprintln!("recap: the overlay can't be kept a tool window");
            }
            let ex = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(window, GWL_EXSTYLE, tool_window_style(ex) as isize);
            let _ = SetWindowPos(
                window,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }

    /// Hide `raw` from screen capture and sharing (`on`), or show it again.
    pub fn exclude_from_capture(raw: isize, on: bool) {
        let affinity = if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        // SAFETY: a window of this process.
        if let Err(error) = unsafe { SetWindowDisplayAffinity(hwnd(raw), affinity) } {
            eprintln!("recap: the overlay's capture setting: {error}");
        }
    }

    /// `raw`'s display affinity (0 none, 0x11 excluded from capture).
    pub fn capture_affinity(raw: isize) -> u32 {
        let mut affinity = 0;
        // SAFETY: a window of this process; writes one u32.
        let _ = unsafe { GetWindowDisplayAffinity(hwnd(raw), &mut affinity) };
        affinity
    }

    /// `raw`'s extended style.
    pub fn ex_style(raw: isize) -> u32 {
        // SAFETY: reads a window long.
        unsafe { GetWindowLongPtrW(hwnd(raw), GWL_EXSTYLE) as u32 }
    }
}

#[cfg(not(windows))]
pub mod win32 {
    pub fn find(_title: &str) -> Option<isize> {
        None
    }
    pub fn keep_tool_window(_raw: isize) {}
    pub fn exclude_from_capture(_raw: isize, _on: bool) {}
}

#[cfg(test)]
mod tests {
    use super::tool_window_style;

    #[test]
    fn a_tool_window_has_no_taskbar_button() {
        // winit's style for a frameless top-level window: APPWINDOW in.
        let winit = 0x0004_0000 | 0x0000_0100 | 0x0000_0010;
        let kept = tool_window_style(winit);
        assert_eq!(kept & 0x0000_0080, 0x80, "WS_EX_TOOLWINDOW");
        assert_eq!(kept & 0x0004_0000, 0, "no WS_EX_APPWINDOW");
        // Click-through's WS_EX_TRANSPARENT | WS_EX_LAYERED stay.
        assert_eq!(tool_window_style(0x20 | 0x8_0000) & (0x20 | 0x8_0000), 0x20 | 0x8_0000);
    }
}
