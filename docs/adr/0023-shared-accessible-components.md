# 0023 Shared accessible components; the overlay draws its own tooltips

## Context

The UI had several copies of the same control: three `bare_button`s (Home footer, Notes footer, the speakers dialog) and a fourth in ConfirmDelete, three menu buttons (`controls.Choice`, the provider picker, the speakers People menu), and icon buttons with plain `tooltip =`. Each copy had its own gaps (see `docs/research/research_accessibility.md`):

- The Actions panel (Ctrl K) was a hand-built `ui.show` box. A click outside didn't close it, focus wasn't trapped or given back, and Esc worked only while focus was inside.
- ConfirmDelete focused Delete first. APG says a dialog that confirms something destructive starts on the least destructive button.
- Home showed an "All meetings" box that looked like a menu and did nothing.
- Hovering a caption-overlay button showed nothing while the user's browser was in front. zgx holds every popup window, tooltips included, while none of the app's windows is active (`Scene::set_popups_held`, `Ui::set_app_active`). The overlay never becomes active, so in a call its tooltips never showed.

The user asked for shared, reusable primitives: one place each, used everywhere. They prefer to build on zgx rather than change it.

## Decision

Each primitive is a thin Luau wrapper over zgx's own parts, in `crates/app/ui/components/`:

| Component | Wraps | Rules it carries |
|---|---|---|
| `button` | `ui.button` | Words and optional keycaps. Looks: bare, filled, outlined, tinted, link. The keys go in `description` (lib/keys: "Ctrl+Enter"), since zgx has no node prop for UIA AcceleratorKey. `autofocus` focuses it a frame after it mounts. |
| `icon_button` | `ui.button` | `label` is the name and the tooltip; `on` is a toggle (`checked`); `expanded` for a trigger. `inline_tip` (below). |
| `select` | `ui.menu` | APG select-only: the chosen label and ⌃⌄; `expanded`; the chosen item checked (`checked` + `radio`) and opened over the trigger (`open_over`). The name reads "label, value". Triggers: value, field (as wide as its row), icon. |
| `popover` | `ui.popover` | Esc and an outside click close it, focus is trapped and given back. Content is passed as children: `build` with a function `open` fails in zgx 24b3587 (`open:get()`). |
| `dialog` | `ui.modal` | Esc closes, focus trapped and given back. `autofocus = false` plus a button's `autofocus` starts on another button. |
| `tabs` | `role = tablist/tab` | One Tab stop with `focus_group`, arrows and Home/End move, `selected`. Manual activation (Enter, Space, click), as Base UI: a tab focused again as its window comes back must not take over a tab the app chose. |
| `switch`, `checkbox` | `ui.button` | `role` and `checked`; the switch's name never changes. |
| `announce` | a 0-size `ui.text` | `role = "status"` / `live = "polite"`, or `alert` / `assertive` for errors. It stays mounted and its text changes. Used for the HUD ("Copied"), and for the overlay's Reconnecting and errors. Nothing changes on screen. |

Uses: the Actions panel is a `popover` anchored above the footer's Actions button (top, end, 8). ConfirmDelete is a `dialog` that starts on Cancel: Enter still deletes, as its ↵ keycap says, and Space or a click on Cancel cancels. The provider sheet is a `dialog`. The Notes tabs and the Settings sidebar are `tabs`. The dead "All meetings" box is removed.

**The overlay's tooltips are drawn inside the overlay.** `icon_button`'s `inline_tip = true` replaces zgx's popup tooltip with a node in the same window: after 300 ms of hover, under the button with its right edge on the button's (the toolbar sits at the overlay's top right, where a centred tip would run off), gone on leave or press. One that follows another within 400 ms shows at once (Base UI's delay group). It is `aria_hidden`: the button's `label` already says it. Only the overlay's toolbar uses it; the other windows are active when the user hovers them.

## Consequences

- One place to fix each control. A new screen gets names, states and keys by using the components.
- No zgx changes. Still missing from zgx: AutomationId (zgx ids don't reach UIA) and AcceleratorKey on plain nodes.
- Tests: `tests/a11y.test.luau` (inline overlay tips, announcements, select check, People list, tabs keys, no All meetings box) and `tests/actions.test.luau` (outside click, panel placement, Delete starting on Cancel).
