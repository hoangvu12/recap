# 0020 The overlay is always on top: no pin; click-through resets each meeting

## Context

The user reported that after starting a new meeting and switching windows, the caption overlay vanished and never came back, while Notes stayed. They also could not tell what the hover-toolbar buttons did, and some seemed to do nothing.

What we found in the installed build:

- **Pin.** The toolbar's "Keep on top" toggle took `WS_EX_TOPMOST` off the overlay. Its state lived in `OverlayHost`, so the next meeting's overlay also opened without topmost. The overlay is a tool window, with no taskbar button and no Alt+Tab entry. Once any normal window was raised over it, there was no way to get it back.
- **Late stream end.** `Host::follow_meeting` handled a stopped meeting's last events after a new meeting had started. The old meeting's stream end closed the new meeting's overlay and cleared its controls.
- **Show notes.** It set `window.notes`, which was already true, so nothing happened.
- **Stop.** It waited for both engines to flush, a few seconds, before anything changed on screen, so it looked dead.
- **Click-through.** Once on, the toolbar can't be reached by pointer. Nothing on screen said how to turn it off.
- **No names.** The buttons had icons only, with no tooltips.

## Decision

- **No pin.** The overlay is always topmost. `set_overlay_pinned` and `toggle_pinned` are removed. To get it out of the way, use Stop, click-through, or move it.
- **Click-through** starts off on every new overlay (`OverlayHost::attach`). While it is on, the overlay header shows the click-through hotkey's keycaps next to the timer.
- **Stop** closes the overlay at once. Processing goes on in Notes.
- **Show notes** (`open_notes`) opens and focuses the Notes window.
- **Each toolbar button** has a tooltip and an accessible name: Pause/Resume, Stop, Show notes, Smaller text, Larger text, Caption me too, Let clicks through.
- **`Host::follow_meeting`** numbers each followed meeting. Events, notices and the stream end of a meeting that is no longer current never touch the live state, the overlay or the controls. Its `Saved` still refreshes the list and starts processing.

## Consequences

- An overlay can't get lost behind other windows. A user who wants it hidden stops the meeting or turns on click-through.
- Tests: `a_meeting_ending_late_leaves_the_next_meeting_alone`, `stop_closes_the_overlay_at_once`, and the overlay UI tests for tooltips, no pin, and the click-through keycaps.
