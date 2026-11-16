# Raycast for Windows: measured design reference

Captured 2026-11-16 from Raycast **2.12.1.0** (Microsoft Store build) on Windows 11 Pro 26200.
Display is **96 DPI (100% scale)**, so every pixel value below is a 1x value. Raycast's own
"Interface Size" setting was left at the default (smallest of three).

How the numbers were obtained:

1. **Pixels.** Screenshots were taken with a DPI-aware `ImageGrab` and sampled with Pillow
   (column/row run-length profiles, glyph ink boxes). Each screenshot has 24 px of padding around
   the window's DWM frame (window origin = image (24,24)).
2. **Translucency.** The empty launcher was shot over solid **#000000, #FFFFFF and #808080**
   backdrops (`measure/bd_*.png`) to separate the material's tint from what shows through.
   The named screenshots use the #808080 backdrop, so the browser that was behind Raycast
   never appears.
3. **Cross-check.** Raycast for Windows is a WebView2 app, and its UI CSS/JS ships unminified
   enough to read in `…\WindowsApps\Raycast.Raycast_2.12.1.0_x64__qypenmj9wpt2a\Raycast\frontend`.
   Every token tagged **(css)** was read from there and matches the sampled pixel to within
   ±3/255. Where the two differ, the pixel value is given and the difference noted.

---

## 1. Colour tokens

Raycast's whole palette comes from **5 theme inputs** (`background`, `backgroundSecondary`,
`foreground`, `selection`, `accent`) plus 7 named hues. Every surface, border and text colour is
`foreground`/`selection` at a fixed opacity step (5/10/20/40/60/80%). This is the single most
reusable idea here.

### Theme inputs (css, bundled themes)

| Input | Raycast Dark | Raycast Light |
|---|---|---|
| background / backgroundSecondary | `#000000` | `#FFFFFF` |
| foreground | `#FFFFFF` | `#000000` |
| selection | `#FFFFFF` | `#000000` |
| accent (theme) | `#4FA3F8` | `#138AF2` |
| red / orange / yellow / green | `#FF6363` `#FF9217` `#FFC531` `#59D499` | `#B12424` `#C75D07` `#F8A300` `#006B4F` |
| blue / purple / magenta | `#56C2FF` `#A485FF` `#CF2F98` | `#138AF2` `#6A3DEC` `#9A1B6E` |
| brand | `#FF6363` | `#FF6363` |

The **live accent is the Windows system accent** when there is one:
`--accent: var(--color-system-accent, var(--theme-accent))`. On this PC (system accent `#4A51A8`)
every switch, link and focus tint rendered as **`#686BC6`** in both themes, a lighter variant of
the system colour. The theme blue (`#56C2FF` dark / `#138AF2` light) is still used for "Pro" badges.

### Semantic tokens: formula (css) and measured sRGB

The measured values are what you get on the reference #808080 backdrop. Over other desktops the
window colour drifts by ±13 (see 1.3).

| Token | Formula (css) | Dark measured | Light measured |
|---|---|---|---|
| window background (launcher) | backdrop `#333C` / `#FFFC` + `bg` 40% | **`#272727`** | **`#F2F2F2`** |
| list row selected | `selection` 10% | **`#3D3D3D`** (white 10%) | **`#D9D9D9`** (black 10%) |
| list row hover | `selection` 5% | ≈`#323232` | ≈`#E6E6E6` |
| separator / hairline | `fg` 10% | `#3C3C3C` | `#D9D9D9` |
| border secondary (fields, keycaps) | `fg` 20% | `#525252` | `#C1C1C1` |
| focus ring | `fg` 50% (30% in some views), 1 px | `#939393` | `#787878` |
| input fill | `#FFFFFF0D` (dark only) | `#323232` | none (`#F2F2F2`) |
| control surface | `fg` 10% | `#3D3D3D` | `#D9D9D9` |
| text primary | `fg` | `#FFFFFF` | `#000000` |
| text secondary (subtitles, accessories, section headers, labels) | `fg` 60% | `#A8A8A8` | `#616161` |
| text tertiary / placeholder / hint | `fg` 40% | `#7E7E7E` | `#919191` |
| text quaternary | `fg` 20% | `#525252` | `#C1C1C1` |
| action panel surface | panel overlay | `#363636`, border `#5E5E5E` | `#F2F2F2`, border `#C2C2C2` |
| action panel selected item | | `#4A4A4A` | `#D9D9D9` |
| "Pro" badge | blue 10% fill, blue text | fill `#2C363D`, text `#55BEFA` | fill `#DBE7F1`, text `#1586EA` |
| switch on / off | `accent` / `fg` 20%; thumb `#FFFFFFE6`→`F2` | `#686BC6` / `#4C4C4C` | `#686BC6` / `#DADADA` |
| scrollbar thumb | `fg` 40%, 6 px → 11 px on hover | | |
| window outer border (DWM) | | `#2F2F2F` over black / `#C8C8C8` over white (≈ grey at 40% alpha) | same |

### 1.3 Translucency: measured

The launcher is an **acrylic-style blurred material with a heavy tint**. It is not opaque, but only
about 10% of the backdrop shows through.

| Backdrop | Dark window bg | Light window bg |
|---|---|---|
| `#000000` | `#1A1A1A` | `#E5E5E5` |
| `#808080` | `#272727` | `#F2F2F2` |
| `#FFFFFF` | `#343434` | `#FFFFFF` |

This is linear in the backdrop (`out ≈ 0.90·T + 0.10·backdrop`). The fit gives T ≈ `#1D1D1D`
(dark) and `#FFFFFF` (light) at 90% opacity. In CSS terms it is `#333` at 80% under black at 40%
(dark), with a "reduced transparency" fallback of opaque `#333` / `#CCC`. Selection and separator
overlays stack on top with constant alpha: 10% white over each backdrop (#313131 / #3D3D3D / #484848).
Against the real browser behind it, the content showed through as a soft blur.

**Settings window** (`#808080` backdrop only, so its translucency wasn't isolated; it looks like Mica):

| Surface | Dark | Light |
|---|---|---|
| title bar strip (38 px) | `#2A2A2A` | `#F1F1F1` |
| sidebar / content canvas | `#1F1F1F` | `#FEFEFE` |
| settings card | `#232323` | `#F8F8F8` |
| card row divider | `#2F2F2F` | `#EBEBEB` |
| sidebar selected item | `#353535` | `#F1F1F1` |
| sidebar/content divider | `#353535` (1 px at x=230) | `#E4E4E4` |
| title-bar bottom hairline | `#404040` | `#DADADA` |

---

## 2. Typography

* **Family: `InterVariable`**, bundled in the app (`InterVariable.woff2`, `JetBrainsMono-Regular.woff2`
  for code). Some CSS paths prefer `Segoe UI Variable, Segoe UI, InterVariable`, but the launcher,
  lists, forms and settings all render as Inter: double-storey `a`, flat-topped `t`, flagged `1`
  (see the zoomed glyphs in the screenshots).
* Rendering: greyscale antialiasing (Chromium/WebView2), no ClearType colour fringes.
* Scale tokens (css): **8, 10, 11, 13, 16, 18, 24 px**, scaled by `--spx` (Interface Size zoom).
* Weights (css): 400 / 500 / 600 / 700.

| Role | Size | Weight | Colour | Measured evidence |
|---|---|---|---|---|
| Search field text and placeholder | 18 px | 400 | primary / tertiary | cap height 13 px |
| List item title | 13 px | 500 | primary | cap height 10 px |
| List item subtitle | 13 px | 400 | secondary | cap 10 px |
| List accessory ("Command", "Application") | 13 px | 400 | secondary | |
| Section header ("Suggestions", "Results") | 11 px | 500 | secondary | cap 8 px |
| Footer primary action ("Open Command") | 13 px | 500–600 | primary | |
| Footer "Actions" label | 13 px | 400 | secondary | |
| Keycap glyph | 10 px | 500 | secondary | (css `data-size=small`) |
| "Pro" badge | 10–11 px | 500 | blue | |
| Form label | 11–12 px | 500 | secondary | cap 8 px |
| Form placeholder | 13 px | 400 | tertiary | |
| Detail metadata label / value | 13 px | 400 / 500 | secondary / primary | |
| Calculator result | 24 px | 600 | primary | |
| Settings row label | 13 px | 400–500 | primary | |
| Settings row description | 11 px | 400 | secondary | |
| Settings page hero title (extension) | 24 px | 600 | primary | |

---

## 3. Spacing, size and radius

Spacing scale (css, × `--spx`): **2, 4, 6, 8, 12, 16, 24, 32, 40, 48, 56**. It's an 8-point grid
with 2/4/6 for tight work.

Radius scale (css): 2, 4, 6, 8, 10, 12, 16, 18, 20, 26, full. **`--radius-window: 8px`** on Windows
(macOS uses 20/26). Measured:

| Element | Radius (measured) |
|---|---|
| Window (DWM-rounded) | 8 px |
| List row selection | 8 px |
| Action panel | 8 px |
| Action panel item | 6 px |
| Form input / dropdown button | 6 px |
| Keycap | 4 px |
| Pro badge, switch, scrollbar | full (pill) |

Icon sizes (css): 12, 16, **22**, 32. List icons are 22 px; settings sidebar icons 22 px in a
rounded tile.

---

## 4. Elevation and motion

* **Panel shadow (css)**: `0 8px 10px -6px #0000001A, 0 20px 25px -5px #00000033`, with a variant of
  `inset 0 0 0 1px fg-20` as a border. Measured around the action panel in light: a roughly 10 px soft
  falloff on the left and below (`#EDEDED → #E1E1E1`). It's barely visible in dark, where the 1 px
  `#5E5E5E` border does the work.
* The window itself has no app-drawn shadow; it relies on the DWM shadow plus the 1 px DWM border.
* **Motion tokens (css)**:
  * hover in 100 ms, hover out 200 ms (HUD), toast hover out 350 ms
  * `--transition-duration: .15s` with `ease` / `ease-out`
  * `--easing-ease-out-expo: cubic-bezier(.16,1,.3,1)`, `--easing-ease-out-back: cubic-bezier(.34,1.56,.64,1)`
  * switch thumb: `transform .366s cubic-bezier(.16,-.12,.29,1.31)` (slight overshoot); track colour `.216s ease-out`
  * scrollbar: fades out over `.3s ease-out`; widens from 6 to 11 px after 2 s of hover (`.18s`)
  * number-hotkey hints appear after a 400 ms hold, over 150 ms
* Observed: list selection moves **instantly** (no slide). The launcher appears and disappears with no
  visible animation at a 120 ms capture interval.

---

## 5. Component anatomy (1x pixels, window-relative)

### Launcher window (`01_*`, `02_*`)
* **750 × 475**, fixed (css `--main-window-width: 750`, `--main-window-expanded-height: 475`).
  It opens centred horizontally on the screen with the mouse, at about 30% from the top.
* Vertical stack: **header 64 px** → 1 px separator → list (366 px) → 1 px separator → **footer 40 px**
  (css 42 with hairlines).
* Header: Raycast logo / back button at x=16 (22 px), search text starting at x=50, 18 px. On the right
  is a "Quick AI" tertiary label plus a `Tab` keycap, with 16 px right padding. There is no box around
  the search field: it's bare text on the window.

### List row
* **Height 38 px**, inset **8 px** from each window edge (selection is 734 px wide), **8 px radius**.
* Inside: 8 px padding → 22 px icon → 12 px gap → title (13/500 primary) → 10–12 px gap → subtitle
  (13/400 secondary). Accessory text is right-aligned with 8 px padding.
* Selected = `selection` 10%, hover = 5%. No left accent bar, no border.
* Optional right badges sit before the accessory: "Pro" pill, 14 px high, blue 10% fill.

### Section header
* **32 px** block (about 24 px plus an 8 px top gap). 11 px / 500 / secondary text at x=16, no rule
  line, sentence case ("Suggestions", "Results", "Use "create snippet" with… ⚙").

### Footer / action bar
* 40 px. On the left is a 16 px hamburger glyph, or the current command's icon and name (13 px
  secondary) inside a command.
* On the right: **primary action label (primary colour, 500) + ↵ keycap**, a 1 px × 14 px vertical
  divider in `fg` 20%, then **"Actions" (secondary) + `Ctrl` `K` keycaps**. When the action panel is
  open, the "Actions Ctrl K" group gets a `control` 10% pill behind it.

### Keycap badge
* **Outlined, no fill**: `inset 0 0 0 1px fg-20`, 4 px radius, **18 px tall**, min-width 18 px, 4 px
  side padding, 2 px gap between caps.
* Glyph 10 px in secondary colour, slashed zero. Words like `Ctrl`, `Shift`, `Alt` and `Tab` are
  spelled out; Enter is shown as ↵.

### Action panel (`03_*`, Ctrl+K)
* Anchored bottom-right above the footer: **350 × 280**, 8 px from the window's right edge and 8 px
  above the footer. 8 px radius, 1 px border (`#5E5E5E` dark / `#C2C2C2` light), panel shadow.
* Content: section title (11 px secondary, "Calculator") → items **32 px tall**, inset 8 px, 6 px radius
  → items have a 16 px icon, 13 px title and right-aligned keycap combos.
* At the bottom: a 1 px separator and a **40 px "Search for actions…" field** (css `menu__searchField`
  height 40, padding 16). There's no box around it.

### Inline calculator card (`07_*`)
* Full-width card, 8 px inset, **124 px tall**, 8 px radius, `control` 10% fill. Two halves split by a
  1 px divider with an → glyph. 24 px / 600 numbers, small 10% "Expression" / "Result" chip labels.

### List + detail (`05_*`, Clipboard History)
* List column **284 px** wide (8–292). The detail pane starts at x=284 after a 1 px `fg` 10% vertical
  divider and runs to the edge.
* The header search bar has a **dropdown accessory on the right**: 197 × 34, 6 px radius, 1 px `fg` 20%
  border, list icon + "All Types" + chevron.
* Detail: preview area on top (fit-to-width image/text, 16 px padding), then an **"Information"
  metadata block**: 11 px secondary heading, then rows **33 px** apart with 1 px `fg` 10% separators.
  Each row has a label on the left (13 px secondary) and the value on the right (13 px primary,
  optional 16 px app icon).
* Footer inside a command: command icon and name on the left ("Clipboard History"), primary action
  "Paste ↵".
* *Clipboard contents are pixelated in the shipped screenshots; only the chrome is real.*

### Form (`06_*`, Create Snippet: opened, never saved)
* Two-column layout with a back button (24 px tile) at top-left and an underlined link button at
  top-right ("Snippets Guide").
* Label above field: 11–12 px / 500 secondary, about 8 px gap to the field.
* **Text field: 34 px tall** (css 32 + border), 6 px radius, 1 px `fg` 20% border. Dark fill is
  `#FFFFFF0D`; light has no fill. 12 px inner padding.
* Focus: border becomes the 1 px **focus ring at `fg` 50%** (`#939393` dark / `#787878` light). There's
  no accent colour and no glow.
* Multiline editor 308 × 274 with a formatting toolbar at the bottom (B, I, U, S, link, clear; 16 px
  glyphs, 1 px divider). The helper text below is 11 px secondary, with an inline link in accent
  colour ("{Dynamic Placeholders}").
* A combined "name + icon" field puts an icon dropdown segment inside the input, split by a 1 px
  divider.
* Footer: command identity on the left, **"Save Snippet" + `Ctrl` `↵`** on the right.

### Settings window (`09`–`13`)
* Separate standard window, **800 × 560**, 8 px DWM corners, custom 38 px title bar with "Settings"
  (12 px secondary) on the left and back/forward chevrons. Minimise and close are 46 px wide caption
  buttons; there's no maximise.
* **Sidebar 230 px**: a search field (32 px, 6 px radius, control fill) → an account row (32 px avatar
  circle) → nav items **32 px tall**, 8 px inset, 6 px radius, 22 px colour icon tiles, 13 px / 500
  labels. The selected item gets the `control` fill. "Pro" pills sit at the right edge. Extensions
  follow the core sections as a flat list.
* Content: 16 px padding. **Grouped cards** with 8 px radius, `#232323` / `#F8F8F8`. Rows are **47 px**
  (single line) or **58 px** (label + 11 px description), split by 1 px dividers inset to the card
  padding. Each row has the label on the left and the control on the right: switch, dropdown text
  button with chevron, "Start Free Trial" bordered button, keycap-style hotkey chip.
* Section titles ("Appearance") sit outside the cards in 13 px / 500 primary, 16 px above the card.
* **Switch: 28 × 16** pill (css `--switch-width: 28`, `--switch-height: 16`), 2 px padding, 12 px white thumb. On = accent; off = `fg` 20%.
* Dropdown menu (`12_*`): popover 196 px wide, 8 px radius, `bg` 80% material, 1 px border. 11 px
  secondary group headers ("Dark Themes"), **32 px items** with 6 px radius selection, separators,
  and a "Search…" field at the bottom (the same pattern as the action panel).
* Extension page (`11_*`): centred 64 px icon, 24 px / 600 title, 13 px secondary description,
  then cards.

### Toast / HUD (from CSS; **not captured**, see below)
* **HUD**: pill-shaped, **46 px tall** (52 in some contexts), padding-inline 24, gap 12, 22 px icon,
  max-width 500, `bg` 40% material. Hover in 100 ms / out 200 ms.
* **Toast** (inside the footer area): **44 px tall**, padding-inline 16, gap 8, max-width 600, a
  status indicator inset 12 px, 24 px action buttons, a 28 px dismiss button. Variant tints are
  red/green/yellow at 26%, as gradients.

### Scrollbar
* Overlay scrollbar with no track: a 6 px pill thumb in `fg` 40%. Hovering widens it to 11 px and
  shows a `fg` 5% track. It fades out after scrolling stops. None was visible at rest in any capture.

---

## 6. What makes it feel clean

1. **One hue, many opacities.** Almost every surface, border and text colour is the foreground at
   5/10/20/40/60%. Nothing clashes, and a theme is just 5 inputs. Copy this structure directly.
2. **No boxes where there's no need.** The search field, the action panel's search and the footer
   have no borders or fills: bare text on the material, with hairlines (`fg` 10%) dividing regions.
3. **Selection is a soft 10% wash with an 8 px radius, inset 8 px.** There's no accent bar, no
   coloured highlight and no border, and it moves instantly. Accent colour is saved for "on" states
   and links.
4. **A strict vertical rhythm.** Rows are 38 (list), 32 (menus), 34 (fields), 47 (settings rows),
   40–42 (footer), 64 (header). Everything sits on a 4/8 px grid with 8 px window insets.
5. **Two-and-a-half type sizes.** 13 px carries almost everything, 11 px for headers and hints, and
   18 px for the single search field. Hierarchy comes from weight (500 vs 400) and opacity
   (100/60/40%), not from size.
6. **Keyboard-first chrome that teaches itself.** Every footer and menu row shows outlined keycaps:
   quiet, 18 px, secondary colour, words spelled out. The primary action is always the bright label
   next to ↵.
7. **Heavy-tint acrylic.** About 90% tint with 10% show-through gives depth without hurting legibility.
   Over any desktop the window stays within about ±13 of its nominal grey.
8. **Native Windows seams.** It uses DWM 8 px corners and the DWM border/shadow, follows the system
   accent, follows the system theme, and has standard caption buttons on Settings. It feels native
   even though it's a web UI.
9. **Calm motion.** It uses 100–200 ms hovers, an ease-out-expo curve and one playful overshoot
   (the switch thumb). Navigation has no slide transitions.

---

## 7. Mapping to the notetaker (suggested starting tokens)

| Token | Dark | Light |
|---|---|---|
| `bg.window` (acrylic tint, 90%) | `#1D1D1D` → renders ≈ `#272727` | `#FFFFFF` → ≈ `#F2F2F2` |
| `bg.window.opaque` (reduced transparency) | `#262626` / `#333333` | `#FFFFFF` / `#CCCCCC` |
| `fg` | `#FFFFFF` | `#000000` |
| `fill.hover / selected / control` | fg 5% / 10% / 10% | same |
| `stroke.separator / field / focus` | fg 10% / 20% / 50% | same |
| `text.primary / secondary / tertiary` | fg 100% / 60% / 40% | same |
| `accent` | Windows system accent (fallback `#4FA3F8`) | system accent (fallback `#138AF2`) |
| `radius.window / row / control / chip` | 8 / 8 / 6 / 4 | |
| `font` | Inter Variable 13/500 rows, 11/500 headers, 18/400 input | |
| `size.row / menuItem / field / footer / header` | 38 / 32 / 32(+2) / 42 / 64 | |

For a **live-caption overlay**, the HUD spec (46 px pill, 24 px padding, 22 px icon, `bg` 40% material)
and the 90% acrylic tint are the closest Raycast precedents.

---

## 8. Not captured, and why

* **HUD / toast:** "Copy Answer" on `12*12` copied `144` (Raycast's own clipboard entry confirms it),
  but no HUD appeared. A 3 s burst capture of both displays saw nothing new, so the HUD is likely
  disabled or suppressed in this setup. The CSS values above stand in.
* **AI Chat:** a Pro command, and Raycast isn't signed in here. Skipped per the brief (login/payment).
* **Settings extension command list:** that page wouldn't scroll without the real mouse. Only the
  extension header and cards were captured.
* **Theme dropdown** (dark only) and **Window Mode row** (light only): each captured in one theme.
* **Quick AI / Ask Browser rows:** seen in lists, not opened (Pro, or acts on the browser).
* **Settings translucency** wasn't isolated (captured over grey only).
* **Scrollbar** never visible at rest; its spec is from CSS.

## Files

| File | Content |
|---|---|
| `01_launcher_{dark,light}` | root search, empty: suggestions, sections, hover row, Pro badge, footer |
| `02_query_{dark,light}` | query "calc", selected row, Results section |
| `03_actions_{dark,light}` | Ctrl+K action panel with keycap combos |
| `04_query_fallback_dark` | "Use … with" fallback section header with gear accessory |
| `05_list_detail_{dark,light}` | Clipboard History list + detail + metadata (content pixelated) |
| `06_form_*`, `06b_form_focus_*` | Create Snippet form, then with the Name field focused |
| `07_calc_inline_{dark,light}` | inline calculator card |
| `09`–`11_settings_*` | Settings: General, Launcher, an extension page |
| `12_settings_dropdown_dark` | Theme dropdown menu (popover) |
| `13_settings_windowmode_light` | Appearance and Window Mode cards |
| `measure/bd_{dark,light}_{000000,ffffff,808080}` | empty launcher over solid backdrops (translucency maths) |

Nothing in Raycast was created, saved or deleted. The theme was switched to fixed Raycast Light for
the light captures, then restored (Follow System Appearance **on**, Dark = Raycast Dark,
Light = Raycast Light). The Settings window was closed again. The only side effect is clipboard
history gaining one entry, `144`, from the copy test.
