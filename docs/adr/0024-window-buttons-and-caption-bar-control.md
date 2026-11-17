# 0024 Every window has Windows' buttons; the caption bar has a ✕, a Captions button and a setting

## Context

Home, Notes and Name speakers were frameless, Raycast-style windows with no title bar buttons. Only Settings had minimize, maximize and close. The user couldn't close Home except with Alt+F4 or the tray. They said: "add them back, i cant close any of them, we not building a raycast alternative. add them to all."

The caption bar (overlay) can't simply get the same three buttons. It is a tool window, with no taskbar button and no Alt+Tab entry, so a minimized one couldn't be brought back (the same bug as ADR 0020). Maximize makes no sense for a 720×120 strip. Asked about it, the user said the caption bar felt out of their control: it opened by itself on Start, and the only way to hide it was a hotkey nothing on screen showed.

## Decision

- **Home, Notes, Name speakers and Settings** use `Titlebar::Inset`: Windows 11's own minimize, maximize and close (138×32 points) over the top right, with snap layouts, system tooltips and accessibility. This drops the Raycast frameless look for these windows.
- **Home and Notes** get a 32-point title strip (`components/title_bar.luau`) with the window's name, under the buttons. It moves the window, and a double click maximizes it. Notes' header moves up to sit under it. Name speakers keeps its 56-point header, its title on the left, clear of the buttons. Settings keeps its 40-point strip.
- **The caption bar** stays without Windows' buttons. It gets its own ✕ at the top right, always shown, named "Hide captions". The ✕ hides the bar; the meeting goes on. The hover toolbar sits left of it.
- **Notes** shows a Captions button (with the captions hotkey in its tooltip) beside Stop while a meeting is live. It shows or hides the caption bar and is highlighted while the bar shows.
- **Settings, Captions:** "Show captions when a meeting starts" (`captions_on_start`, on by default). Off, Start opens only Notes.

## Consequences

- Every window can be closed, minimized and maximized the usual way. Closing still leaves Recap in the tray (`Quit::Resident`). Closing Notes during a meeting doesn't stop it.
- Home and Notes are 32 points taller for the same content.
- The caption bar can be hidden and brought back without a hotkey.
- Tests: the window option tests in `windows.rs`, the storage defaults test, and the UI tests for the ✕, the Captions button and the setting.
