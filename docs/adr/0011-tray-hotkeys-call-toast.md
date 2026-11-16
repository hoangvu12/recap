# 0011: Recap lives in the tray; hotkeys, the call toast and autostart

## Context

Ticket #15: a tray icon, global hotkeys, a toast when a call takes the mic,
one instance, and autostart. zgx has toasts and single instance (its
application id), but no tray and no global hotkeys.

## Decision

- **One set of commands** (`recap::commands`): the tray menu, the hotkeys
  and the toasts all run `Command`s on the host, so each does the same thing
  wherever it is asked from.
- **Start notes hotkey toggles:** idle it starts notes, live it stops them
  (the ticket: "starts/stops notes"). Pressed twice it never starts two.
- **Quit stops first:** Quit while recording stops the meeting and quits once
  it is saved (`Host::when_idle`).
- **Closing the last window no longer quits** (`Quit::Resident`); the tray's
  Quit does. A left click on the icon opens Home. The icon is a ring at rest,
  the rec red dot while notes are live.
- **Tray and hotkeys:** `tray-icon` 0.26 and `global-hotkey` 0.9, made on the
  UI thread; their event handlers forward into tokio channels awaited by zgx
  tasks. Hotkeys are registered as Settings has them, again on each change,
  and rest while Settings records a new one (`hotkey_recording` is shared),
  so the keys reach Settings. One another app holds is saved but reported
  under its row ("Another app uses Ctrl+Alt+O", shared `hotkey_conflicts`).
- **Call toast:** a thread polls every second: active capture sessions
  (wasapi) plus the ConsentStore's in-use apps, the latter only while some
  other capture session is active (a crash can leave an entry "in use").
  `CallWatch` turns that into hints: a known meeting app taking the mic
  (Teams, Zoom, Webex, Slack, Discord, browsers) suggests Start once per
  use; during notes, the call's app off the mic for 60 s suggests Stop once.
  Notes started without a call app are never offered Stop. The toast is
  posted from Luau (`ui.notify`, `toasts.luau`); only a click acts.
- **Single instance:** `Application::application_id("app.recap.Recap")`. A
  second launch reopens the first (Home comes forward) and exits. The id is
  also the toasts' AppUserModelID (zgx registers it under
  `HKCU\Software\Classes\AppUserModelId` on the first toast).
- **Autostart:** the value `Recap` under `HKCU\...\Run`, `"<exe>" --background`
  (tray only), written when the switch is on and removed when off.
- **Capture devices (from #7):** `LiveCapture::start_with(CaptureOptions)`.
  A chosen mic that isn't there falls back to the default; a chosen meeting
  app that isn't running falls back to all system audio. Either is shown in
  the notice line. An app's audio is its root process tree (include-tree
  process loopback).

## Consequences

- The overlay owns applying click-through: the hotkey flips the shared
  `overlay.click_through` (also the host action `toggle_click_through`).
- The polling thread costs one WASAPI enumeration a second, only while the
  call toast is on.
- Browser calls are detected per browser, not per tab.
