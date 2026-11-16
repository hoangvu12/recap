# Motion and live-text spec (from Wispr Flow Notetaker, adapted to our Raycast tokens)

Date: 2026-11-16. Wispr Flow v1.6.1483 (Windows).

**Method.** This is a static read of `wispr_extract\.webpack\renderer\hub\index.js`, the bundle for the notes window (Notepad). Its CSS modules are compiled into the JS as strings. Class names are hashed, so the readable names below come from each module's `locals` map. I also used the screenshots in `ux_study\shots\`. Wispr was not run. Positions are character offsets in the one-line bundle (`@3320417` = byte offset), so you can find them with a search.

**Labels.** **C** = confirmed in the code or a screenshot. **I** = inferred.

---

## 1. What Wispr does

### 1.1 Live transcript text
| Item | Wispr value | Source | |
|---|---|---|---|
| Final text | Bubble: `background:var(--background-tertiary)`, `color:var(--foreground)`, 14 px / 500 / line-height 1.5, padding 8×12, radius 12 px. The corner on the speaker side is 4 px for first, middle and last bubbles (`data-pos=first/middle/last`) | `.transcriptBubble`, hub CSS module @5241427 | C |
| Partial text inside a bubble that is already on screen | The final text and the partial text are **two inline spans in the same bubble**: `.bubbleFinalText` + `.bubbleInterimText{color:var(--foreground-tertiary)}`. No italics, no cursor, no shimmer | MZe "TranscriptBubbleRow" @3297848; CSS ibid. | C |
| Partial text when no bubble exists yet (a new turn) | A separate "standalone" bubble in the list footer: `.transcriptBubble.transcriptBubbleInterim{opacity:.5}`. The whole bubble is at 50%, so it looks like the "grey bubble" | `Wt` footer @3320417 (`data-testid=transcript-list-interim`) | C (screenshot `04_live_1m35.jpg`, last bubble) |
| How new words appear | **No animation.** The span's text is just replaced on each partial. There is no per-word fade, blur or slide | MZe renders plain strings; no animation on `.bubbleInterimText` | C |
| Partial → final | **Instant colour swap.** The text moves from the interim span to the final span, with no transition, so it snaps from tertiary to primary. The bubble keeps its box and just grows as text wraps | same | C |
| Empty / waiting text | "Listening…" (`ft`) | @3313049 | C |
| Pause marker | A row with a dashed hairline: `Paused 3:54pm – Resumed …`, with `.transcriptMarkerLine{border-top:1px dashed var(--border)}` and 10 px vertical padding | `AZe` TranscriptGapRow; CSS ibid. | C |

### 1.2 Scrolling
| Item | Wispr value | Source | |
|---|---|---|---|
| List | A virtualised list (react-virtuoso) with `followOutput`, `atBottomStateChange`, `initialTopMostItemIndex = LAST, align end` | @3312551, `gZe={index:"LAST",align:"end"}` | C |
| Follow the bottom | `followOutput` returns `"auto"`, which is an **instant** scroll (not smooth), and only while pinned. It is pinned when the list is at the bottom and auto-scroll is on (off while searching or editing) | hook at @3312551 | C |
| User scrolls up | The list stops pinning. `overflow-anchor:none`, `scroll-padding-block:24px` | `.transcriptList` | C |
| Top edge fade once scrolled | `[data-scrolled]{mask-image:linear-gradient(to bottom, transparent 0, #000 24px)}` | `.transcriptList[data-scrolled]` | C (visible in `04_live_1m35.jpg`) |
| "Jump to bottom" | A 32 px circle, white, 1 px border, `shadow-md`, down-arrow icon 16 px. It sits bottom-right (`right:16px; bottom:max(obstruction+24px,158px)`). It shows whenever the list is not pinned. **Enter:** opacity 0→1 and y 8→0 over **150 ms easeOut**. **Exit:** the reverse. **Reduced motion:** duration 0 | `mKe` @3124469; CSS module 39947; `.scrollToBottomButton` | C |
| Click "Jump" | `scrollToIndex(LAST, behavior:"auto")`, an **instant** jump | `lt()` called with no argument | C |
| Keyboard ↑/↓ | `scrollBy 40px`, `smooth`, or `auto` under reduced motion | @3315257 | C |

### 1.3 New speaker / new bubble
| Item | Wispr value | Source | |
|---|---|---|---|
| New bubble for the same speaker | Appears instantly (no animation on `.transcriptBubbleRow`) | CSS module | C |
| New speaker group | The group gets `data-arriving` for **600 ms** (`setTimeout(...,600)`), only when a partial "preview" group turns into a real group. Its header row plays `transcript-header-arrive` **280 ms cubic-bezier(0.2,0,0,1)**: from `margin-top: calc(-1lh - 20px); opacity:0`. In practice the header slides down out of the bubble above. **Reduced motion:** `animation:none` | `Fe` set @3307106; CSS `.transcriptGroup[data-arriving]>.transcriptSpeakerRow` | C |
| Stagger | None | | C |
| Group spacing | `gap:6px` inside a group, 16 px between list items, 4 px overlap between consecutive bubbles (`margin-top:-4px`) | CSS | C |

### 1.4 Recording state
| Item | Wispr value | Source | |
|---|---|---|---|
| Recording indicator | The Transcript tab gets a leading **3-bar live waveform** (`barCount:3, bulgeCoefficient:22`), driven by the live audio level and shown only while `recording`. The bars are 2 px wide and animate `transform` (scale) with `will-change:transform`. It is not a pulse | tab list @3401374; `VVe` waveform; bar CSS @4257661 | C (green bars in `04_live_1m35.jpg`) |
| Stop control | Round pill with a 16 px green stop square. The label collapses with `transition: all 100ms linear` and the gap with `gap 100ms linear`. "Finishing…" shows a spinner `spin 1s linear infinite`, which is `none` under reduced motion | `rKe` @3124469; pill CSS | C |
| Timer | `M:SS` in the transcript header (`1:50`), with tabular numbers. No animation | @3342250; shot 04 | C |
| Pause / resume | Stop is a pause. Resume adds the dashed "Paused … – Resumed …" row. No other animation | `AZe` | C |
| Window blurred | Every looping animation gets `animation-play-state:paused` under `:root[data-window-blurred]` | caret, dots CSS | C |

### 1.5 After Stop
| Item | Wispr value | Source | |
|---|---|---|---|
| Processing card ("band") | min-height 120, radius 12, `background-secondary`. On the left, one line of status text plus a "Step n of 3" counter. On the right, an illustration (`art`: lines drawing, pills lifting, a TL;DR chip) | `eXe` @3223948; CSS module 9911 | C (`09_summary_step1_upload.jpg`) |
| Step change | Each new step label is a new React key, so it re-mounts and plays `.progressEnter`: **420 ms cubic-bezier(0.22,1,0.36,1)**, from `opacity:0; translateY(10px)`. **Reduced motion:** the class is not applied | ibid. | C |
| Step done | A ✓ icon (14 px, stroke 2.75) + "Speakers identified" / "Summary ready", in `--green-800` (`--success` in dark). It enters with the same 420 ms rise | ibid. | C |
| Steps | Wispr shows one line at a time, not a 3-row checklist. The steps are upload → transcribe ("Identifying who said what…") → summarize ("Polishing your summary…") | i18n keys | C |
| Errors / slow network | Warning icon (`--orange-700`), "Slow connection" / unreachable text, Retry button (xs, secondary) | ibid. | C |
| Card exit | framer `exit:{height:0, opacity:0}` **300 ms easeOut**. **Reduced motion:** opacity only, duration 0 | `rXe` @3225242 | C |
| Summary appears, path 1 ("refine"): block reveal | The Markdown is split into blocks at headings. Each block animates `opacity 0→1`, `blur(6px)→0`, `translateY(6px)→0` over **520 ms, ease [.22,1,.36,1]**. Block *i* starts at `520 + i·min(340, 5160/(n−1))` ms, so the whole reveal takes at most about 5.7 s. The view auto-scrolls to keep the newest block visible (**620 ms, [.33,1,.68,1]**), then scrolls back to the top (**900 ms, [.65,0,.35,1]**). Lines that are new compared with the quick summary get a background "wash" (`refined-line-wash` **900 ms ease-out**: transparent → `background-secondary` at 30% → transparent). A wheel or pointer-down skips to the end. When the reveal ends, the real editor fades in over 200 ms (`--summary-reveal-settle-duration:200ms`) and the overlay is removed after 240 ms | `AXe`, `oXe`, module 8696 | C |
| Summary appears, path 2: word write-in | Each word is a `.bword` span. The pace is **180 chars/s**, with the total capped at **8 s**. Each word fades `opacity 0→1, blur(5px)→0` in 60–200 ms (about 100 ms), ease [.33,1,.68,1]. A 2 px caret (`--glow-300`) blinks with **720 ms step-end** (to 25% opacity). Bullet markers appear when their first word lands. **Reduced motion:** the overlay is `display:none` and the text shows at once | `HXe`, `Nqe`, module 13208 | C |
| Which path runs | Path 1 when a quick summary exists and is being replaced by the refined one; path 2 for a fresh body | `rXe`/`PQe` props | I |
| Summary tab | The sparkle icon spins when the tab is activated. The label shimmers while the summary is generating (`labelShimmering`, a shared `shimmer-sweep` keyframe: background-position 150%→−150%) | tab list @3401374; shared keyframes | C (props, keyframe) / I (shimmer duration not found) |
| Transcript refining | The panel pulses `opacity 1↔.55`, **2 s ease-in-out infinite**. **Reduced motion:** static at .7 | module 43492 `panelWrapPulsing` | C |
| Transcript loading skeleton | Grey bars: speaker 84×12, lines 10 px tall, radius 4, gap 20/8 | module 39879 | C (no shimmer on the skeleton itself) |
| Toast "3 min meeting compressed to a 2 min read." | One of 4 random messages. A pill with an emoji, `aria-live=polite`. Enter: **opacity 0, y 16 → 0, 350 ms ease [.16,1,.3,1]**. Exit: y −8. Auto-dismiss after **6 s**. **Reduced motion:** opacity only, duration 0 | `Cqe` toast near `Nqe`; hook `celebrating` | C |
| Celebration bar (other surface) | `bar-enter` 320 ms cubic-bezier(.2,.9,.24,1); emoji `celebrate-pop` 520 ms with scale 0.6→1.12→1 and rotate −14°→4°; label swap 260 ms; 3 dots pulse 1.4 s with .2 s stagger | module 19107 | C |
| Read time | "N MIN READ" = ceil(words/200). The row collapses with `grid-template-rows .3s ease-out, opacity .3s` | `Be`; reveal CSS | C |
| Speaker rename | A dialog with sample quotes and a combobox (`14_rename_speakers.jpg`). Speaker chips show a caret on hover (`opacity 120ms ease`). I found no special dialog motion; it likely uses the Base UI popup defaults | `.labelSpeakerCaret` | C (caret) / I (dialog) |

### 1.6 Shared tokens
- **There are no motion CSS variables.** Durations and curves are hard-coded per module. The ones used most:
  - `cubic-bezier(0.4,0,0.2,1)` ×15
  - `cubic-bezier(0.05,0.6,0.4,0.95)` ×15
  - **`cubic-bezier(0.22,1,0.36,1)` ×13** (their "enter" curve)
  - `cubic-bezier(0.2,0.9,0.24,1)` ×4
  - durations 100, 120, 150 and 200 ms for hover and fades
  - 280–520 ms for entrances
- Every CSS module carries the same keyframes:
  - `spin`, `fade-in`, `fade-out`, `rainbow-gradient-wave`, `shimmer-sweep`, `rainbow-pill-ring-spin`
  - `pulse` (50% → opacity .05)
- **Reduced motion is respected throughout:**
  - 64 `prefers-reduced-motion` blocks in the hub bundle;
  - a JS check, `sy.I()` / `matchMedia`, sets framer durations to 0 and drops transforms;
  - loops pause when the window loses focus.

---

## 2. Our spec

### 2.0 Motion tokens (extends DESIGN.md §9.2)
| Token | Value | Use |
|---|---|---|
| `motion.instant` | 0 ms | partial text updates, list selection, follow-scroll |
| `motion.fast` | 100 ms in / 200 ms out, `ease.out` | hover, colour swaps |
| `motion.base` | 150 ms, `ease.outExpo` | fades, pill and toolbar in/out |
| `motion.enter` | 200 ms, `ease.outExpo` | new speaker row, step change, toast |
| `motion.reveal` | 240 ms, `ease.outExpo` | summary sections |
| `motion.scroll` | 200 ms, `ease.outExpo` | explicit jumps only |
| `ease.out` | `cubic-bezier(0,0,.2,1)` | |
| `ease.outExpo` | `cubic-bezier(.16,1,.3,1)` (Raycast) | |
| `rise` | 4 px translateY | every entrance offset (Wispr uses 6–16 px; we stay calmer) |

**Rules:**
- Animate only `opacity`, colour and `transform` (translate and scale). Never animate height, margin or blur.
- A row's height changes instantly. Rows below move with the follow-scroll and are never tweened.
- **Reduced motion:** when Windows "Animation effects" is off (SPI_GETCLIENTAREAANIMATION), or our own setting is on:
  - every transform becomes 0;
  - every duration becomes 0, except opacity fades of ≥150 ms, which become a 100 ms crossfade;
  - loops stop.
- Pause every loop (spinner, rec dot, level bars) when the window is hidden or minimised. The overlay is always visible, so its loops keep running.

### 2.1 Live text: partial → final
**States:**

| State | Notes panel (13/400) | Caption overlay (18–24 px) |
|---|---|---|
| partial (in a row that exists) | Final words `text.primary`, then partial words `text.tertiary` (fg 40%) in the same paragraph, in one text block with two colour runs | Same, on the newest line |
| partial (a new turn, no row yet) | A new row with chip + text all in `text.tertiary`. No fill, no bubble | A new bottom line in tertiary |
| final | `text.primary` | `text.primary` (Them); Me lines `text.secondary` |
| entering (new row) | See 2.3 | See 2.5 |
| error / reconnecting | A one-line status row "Reconnecting…" in `text.tertiary`. On recovery it is replaced instantly. On `auth_error` / `quota` it becomes a red `rec` banner | Same text, dimmed to tertiary, on the bottom line |

**Motion:**
- **Partial updates: no animation** (`motion.instant`). Partials arrive every 100–300 ms, so any fade would flicker.
  - The text block is re-laid out in place.
  - The words already shown don't move unless the STT engine revises them.
- **Partial → final:** colour tween tertiary → primary, **150 ms `ease.out`**, only on the words that were partial.
  - Run it as one colour animation over the partial run, then merge the runs. There is no reflow because the text stays the same.
  - If the final text differs from the last partial (a revised word): swap the text instantly and then run the same colour tween. Don't crossfade two text layers; it ghosts at 13 px.
- No cursor, shimmer, italics or per-word blur in live text. A Raycast-calm UI reads stability as quality.
- **Reduced motion:** the colour swap is instant.
- ⚠ zgx check: `research_zgx.md` lists "no inline nodes". If a single text block can't hold two colour runs, render partial text as a second text node that wraps right after the final one. Otherwise put the partial on its own line inside the row; that is acceptable in the panel, but not in the overlay.

### 2.2 Scrolling (notes panel)
- `ui.list{ measured=true, follow=true, key=segment_id }`. Following is **instant** (`motion.instant`), as in Wispr. A smooth follow lags behind at speech speed and makes text "swim".
- **Pinned** = within 24 px of the bottom. The user scrolling up by more than 24 px unpins it. Typing in Notes or searching also pauses following.
- **When unpinned:** a 24 px top-edge fade mask (fg → transparent) shows content above. Use it only when scrolled, as Wispr does.
- **"Jump to live" pill.** Not a white circle; it is Raycast-style:
  - 28 px tall, `bg.raised` + 1 px `stroke.field`, popover shadow;
  - text "↓ Live" 11/500 `text.secondary`;
  - centred 12 px above the footer hairline.
  - If new rows arrived while unpinned, the label becomes "↓ N new" and a 6 px accent dot appears on the left.
  - Enter: opacity 0→1 + translateY 4→0, `motion.base` (150 ms `ease.outExpo`). Exit: opacity only, 100 ms.
  - Click or `End`: **a smooth scroll of 200 ms `ease.outExpo`** when the distance is ≤ 2 viewports, otherwise an instant jump. Then re-pin. (Reduced motion: always instant.)
- Keyboard ↑/↓ scroll 40 px. Smooth only if reduced motion is off.

### 2.3 New speaker / new row (notes panel)
- **Row layout:** chip (hue at 10% fill, 11/500) + time 11 mono tertiary, then the text below. No bubbles (DESIGN §9.3). Consecutive turns by the same speaker add text rows without a new chip.
- **New row enters:** opacity 0→1 over **`motion.enter` 200 ms `ease.outExpo`**, plus translateY 4→0 on the row's content only. The row's space is taken instantly and the follow-scroll absorbs it.
  - Don't copy Wispr's `margin-top` slide; it animates layout.
- **Speaker change** (new chip): same as above. The chip itself also fades from its hue at 0% fill to 10% over the same 200 ms. No stagger.
- **Pause marker:**
  - a 1 px dashed `stroke.hairline` row with a pause icon and the gap, e.g. "15:54 – 15:58" (11/500 tertiary mono) centred;
  - it appears with a 150 ms fade.
- If more than 3 rows arrive in one frame (catch-up after a reconnect or a long scroll-back): **no** entrance animation; insert them instantly.

### 2.4 Recording state
- **Indicator:** a 6 px `rec` dot + the timer `M:SS` / `H:MM:SS` in JetBrains Mono 11, `text.secondary`, tabular numbers. It sits in the notes footer (left) and in the overlay (left).
  - Dot "breath": opacity 1 → .45 → 1, **2 s `ease-in-out` infinite**. Opacity only, no scale, no ring.
  - **Reduced motion:** a solid dot.
- **Level (optional, notes footer only):** 3 bars, each 2×(4–12) px, `text.tertiary`.
  - Animate `scaleY` only, from the RMS of the live audio. Sample at 30 Hz and smooth with attack 50 ms / release 200 ms.
  - When silent (< −50 dBFS) the bars rest at 4 px.
  - Not in the overlay; it competes with the text.
  - **Reduced motion:** static bars.
- **Pause:**
  - the dot turns `text.tertiary` and stops breathing (150 ms colour change);
  - the timer freezes and shows "Paused" (11/500 tertiary) after it;
  - the footer primary action becomes "Resume ↵".
  - The label swap is an instant text change plus a 100 ms opacity fade; no width tween.
- **Resume:** reverse the above. The breath starts again from its full-opacity phase.

### 2.5 Caption overlay (1–3 lines, newest at the bottom)
- **Layout:**
  - fixed height for N lines (user setting 1–3), with lines bottom-aligned;
  - the bottom line holds the current turn, final words + partial;
  - lines above are earlier finished text, `text.primary` at full opacity for Them and `text.secondary` for Me;
  - the oldest line fades to `text.tertiary` when N = 3, as a depth cue.
- **Line wrap / new line:** when the bottom line wraps or a new turn starts, all lines shift up one line height.
  - Do this with **translateY by one line-height, 150 ms `ease.outExpo`**, and fade the line leaving the top over 120 ms.
  - Move the whole text block as one transform. Never re-flow mid-tween.
  - If a new shift arrives during one, jump the running shift to its end first; don't queue them.
  - **Reduced motion:** instant shift, no fade.
- **Partial behaviour:**
  - Same as 2.1, but the partial colour is fg 55% (not 40%) for contrast on acrylic at 18–24 px.
  - **Hold-back rule:** if the engine revises the partial's earlier words, update in place. Never shift lines up because of a partial alone. Shift only when the *final* text wraps, or when a partial grows past the right edge.
- **New speaker:**
  - no chip in the overlay;
  - a 2 px gap and a hairline "—" lead-in on the first word in `text.tertiary`, instant.
  - Me vs Them differ only by colour.
- **Hover toolbar:**
  - opacity 0→1 + translateY 4→0, `motion.fast` (100 ms in, 200 ms out);
  - it shows after a 150 ms hover delay so it doesn't flash when the mouse crosses the overlay.
- **Idle (no speech for 8 s):** the text fades to `text.tertiary` over 400 ms `ease.out`, and back to normal instantly on the next partial.

### 2.6 After Stop: processing card
**Layout:** a card (`bg.card`, radius 8, padding 16) with **3 rows of 32 px** (DESIGN §9.3), which is more informative than Wispr's single line:
1. Transcribing
2. Naming speakers
3. Writing summary

**Row states:**

| State | Icon (16 px) | Label (13 px) | Right accessory |
|---|---|---|---|
| pending | 6 px dot at `text.quaternary` | `text.tertiary` | — |
| running | spinner: 270° arc, `text.secondary`, **rotate 800 ms linear infinite** | `text.primary` 500 | elapsed `0:12` mono 11 tertiary (only after 5 s) |
| done | ✓ in `text.secondary` (no green: colour is for speakers, rec, accent and errors only) | `text.secondary` | duration `11.5 s` mono 11 quaternary |
| error | ✗ in `rec` | `text.primary` + a second line with the error text 11 `text.secondary` | "Retry ↵" ghost button |
| skipped (e.g. no LLM) | — dash, `text.quaternary` | `text.tertiary` | "Skipped" |

**Transitions:**
- **pending → running:** the label colour moves tertiary → primary, and the spinner fades in, over 150 ms `ease.out`.
- **running → done:**
  - the spinner crossfades to ✓ over 150 ms: ✓ opacity 0→1 and scale .8→1, `ease.outExpo`;
  - the label colour moves to secondary over 200 ms;
  - the next row starts running at the same moment (no gap and no delay).
  - Don't use a bounce or overshoot.
- **→ error:** ✗ crossfades in over 150 ms. The error line enters with opacity only. Retry gets keyboard focus.
- **All done:**
  1. hold 400 ms, so the user sees three ✓;
  2. the card fades out over 150 ms;
  3. the Summary tab content fades in over 200 ms.
  - No height collapse animation: the card's space is replaced in the same frame by the summary, which starts at the top.
- **Tabs while processing:**
  - the Summary tab label shows a tiny spinner (12 px) instead of a shimmer;
  - the Transcript tab label shows "Refining…" in tertiary;
  - the live transcript stays readable with no pulsing (we don't copy Wispr's 2 s opacity pulse).
- **Reduced motion:**
  - the spinner becomes a static "…" glyph cycling every 600 ms, or an indeterminate 2 px bar that doesn't move;
  - all crossfades become instant.

### 2.7 Summary appears
- **No typewriter and no blur write-in.** The summary arrives as one JSON → Markdown result.
- **Reveal section by section:**
  - Overview, each topic, Next steps and Decisions are sections. Each one does opacity 0→1 + translateY 4→0 over **`motion.reveal` 240 ms `ease.outExpo`**.
  - Stagger is **40 ms** per section, capped at 6 sections. Later sections appear with the 6th. The whole reveal is ≤ 500 ms.
  - The view stays at the top; no auto-scroll.
  - Any key, wheel or click ends the reveal at once.
  - **Reduced motion:** a single 100 ms fade of the whole view.
- **Regenerate after a rename:**
  - unchanged text doesn't animate;
  - changed paragraphs get a background wash `fill.selected` (fg 10%) → transparent over **600 ms `ease.out`**, starting at full;
  - nothing moves.
- **Read time:** "2 MIN READ" header, 11/500 secondary, appears with the first section.

### 2.8 Toasts
- **In-window toast** (44 px, footer area, DESIGN §9.3):
  - enter with opacity 0→1 + translateY 4→0, 200 ms `ease.outExpo`;
  - exit with opacity → 0 over 150 ms `ease.out`;
  - auto-dismiss after **4 s** (6 s if it contains an action);
  - the timer pauses while hovered;
  - new toasts replace the current one instantly; they don't stack.
- **"Summary ready"** toast: ✓ icon + "Summary ready" + "2 min read" as a quiet tertiary accessory. No emoji, no pop or rotate, no random copy, no "X · Y · Z" strings.
  - Only shown if the user is not already on the Summary tab.
  - If the notes window is hidden, use a Windows toast instead.
- **"Copied"** uses the HUD pill style: 1.2 s, fade only.

### 2.9 Speaker rename popover / dialog
- **Chip click (inline rename):**
  - a popover (`bg.raised`, popover shadow) opens anchored under the chip;
  - opacity 0→1 + scale .98→1 from the chip edge, **150 ms `ease.outExpo`**;
  - it closes with opacity over 100 ms;
  - the field is focused at once.
- **Assign speakers dialog:**
  - the scrim fades fg 0 → black 40% over 150 ms;
  - the dialog fades opacity 0→1 + translateY 4→0 over 200 ms `ease.outExpo`;
  - it closes with opacity over 150 ms.
- **▶ clip playing:** the icon swaps to ■ instantly. A 2 px progress line under the quote moves with `scaleX` (linear, matching playback).
- **Save names:**
  - renamed chips across the transcript change text instantly;
  - their hue fill flashes from 20% to 10% over 300 ms `ease.out` (one pulse, no loop);
  - then the summary regenerates as in 2.7.
- **Hover caret on a chip:** opacity 100 ms in / 200 ms out (Raycast hover token; Wispr uses 120 ms).

---

## 3. What not to copy

1. **The word write-in with blur and a blinking caret** (180 chars/s, up to 8 s, blur 5–6 px). It is theatrical and slow, blur is costly to render, and it hides content the user is waiting for. Use a ≤ 500 ms section fade.
2. **Celebration motion:** the emoji pop with rotate (−14°→4°, scale 1.12), random "celebration" copy, the rainbow gradient wave and the rainbow ring spin. Raycast is quiet; we use one factual toast.
3. **Animating layout:** Wispr's speaker header slides via `margin-top`, the card collapses via `height`, the read-time row uses `grid-template-rows`. We animate only opacity, colour and transform, and let the space change instantly.
4. **The 2 s opacity pulse on the whole transcript while refining.** It makes text you're reading flicker. Use a tab-label spinner instead.
5. **Chat bubbles with 12 px radius and a fill per line.** They're too heavy for a 420 px Raycast panel. Use chip + plain text rows, with wash only for selection.
6. **The white floating circle with shadow-md for "jump to bottom".** It's off-system for us. Use the small `bg.raised` "↓ Live" pill.
7. **Large entrance offsets** (10 px progress rise, 16 px toast rise, 420–520 ms). Our cap is 4 px and ≤ 240 ms.
8. **The 0.5-opacity grey bubble for a new partial turn.** Opacity on a whole container also dims the chip, and it reads as "disabled" on acrylic. Use `text.tertiary` on the text only.
9. **Two different follow behaviours** (instant follow, smooth keyboard). Keep it consistent: following is instant, and only explicit jumps are smooth.

**What to keep from Wispr:**
- instant partial updates;
- instant follow-scroll;
- partial and final in the same paragraph with only a colour difference;
- the 600 ms "arriving" window idea;
- framer-style reduced-motion handling (duration 0, no transforms);
- pausing loops when the window is unfocused.
