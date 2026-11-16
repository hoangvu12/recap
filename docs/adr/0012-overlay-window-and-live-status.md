# 0012 Overlay window: tool window, capture exclusion, live status

## Context

Ticket #8 makes the caption overlay behave: topmost, no taskbar button, hidden from screen share, click-through, and states for reconnecting and for a live engine that fails mid-meeting with "Switch to Deepgram". Two things got in the way. zgx's winit rewrites `GWL_EXSTYLE` (adding `WS_EX_APPWINDOW` back) every time a window flag changes, so setting `WS_EX_TOOLWINDOW` once doesn't last. And before this ticket a live provider's failure was only reported at Stop, so the overlay couldn't show it.

## Decision

- **Tool window.** `recap::overlay::OverlayHost` finds the overlay's HWND by title on the UI thread (`EnumThreadWindows`) and subclasses it (`SetWindowSubclass`). On `WM_STYLECHANGING` for `GWL_EXSTYLE` it adds `WS_EX_TOOLWINDOW` and removes `WS_EX_APPWINDOW`, so later rewrites by winit keep the style. The overlay opens hidden (`visible: false`) and is shown only after this, so it never flashes a taskbar button.
- **Screen share.** `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` while `hide_from_share` is on (`WDA_NONE` when off), applied at open and on every settings change.
- **Click-through and pin** are window state, not settings. They live in shared state `overlay` `{ click_through, pinned }` and reset each run. `OverlayHost::toggle_click_through()` is the function the global hotkey (#15) calls.
- **Live status.** `LiveStt::run_reporting` takes a `LiveStatus` side channel (`Using(name)`, `Reconnecting`, `Connected`). Its default implementation ignores it. The engine emits `EngineEvent::Live { source, status }`, and an `EngineEvent::Error` for a side as soon as its provider fails. Recording goes on either way. `MeetingControl::replace_live` restarts only the sides that failed with a new provider, and `caption_me` turns my captions on or off mid-meeting.
- **Switch to engine.** In the overlay's error state, "Switch to X" offers the first eligible Live provider that isn't the current one and hasn't been tried. The switch lasts for this meeting only. It doesn't change the Live job's main provider in Settings. A switch that fails becomes the new error and isn't offered again.

## Consequences

- The overlay's Win32 code is in one module (`crates/app/src/overlay.rs`, `win32` inside), with stubs off Windows.
- A provider that doesn't implement `run_reporting` shows Live, but never Reconnecting. ElevenLabs reports both.
- Only the Them side drives the overlay's status. A failure on my side is a notice and doesn't trigger the error state.
