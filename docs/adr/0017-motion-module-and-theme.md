# 0017: One motion module, bindings only; the theme follows Settings; the overlay is scoped dark

## Context
motion_spec §2 gives motion tokens and a reduced-motion rule: with Windows "Animation effects" off there are no transforms, fades of 150 ms or more become 100 ms, everything else is instant, and loops stand still. Screens animated ad hoc with `anime.animate`/`ui.loop`. Settings stores `theme` (system/dark/light), but nothing applied it. The overlay must stay dark.

zgx has two kinds of animation. Bindings (`ui.tween`, `ui.loop`) are played by each window on its own frames. The anime engine (`anime.animate`, `anime.utils`, and the `enter`/`exit` options of `ui.presence`/`ui.each`) is ticked on the root window's frames. Recap's root window is hidden, so anime never runs there. In the real app, an `on_mount` fade stayed at opacity 0 (the Summary tab was blank, and new transcript rows were invisible), and a presence exit never finished. Headless `zgx test` doesn't show this.

## Decision
- `ui/lib/motion.luau` holds the tokens and is the only way the UI animates. Everything in it is a binding:
  - `Enter(kind, props, m)`: a node that plays a Motion as it mounts.
  - `Presence(cond, build, {exit, box})`: our own presence, whose exit is an opacity tween on a box.
  - `entrance(play, m)`
  - `tween`
  - `color`
  - `Looping`

  Recap uses no anime at all.
- The module applies the reduced-motion rule itself (`motion.plan`, which is pure). It reads `ui.prefers.reduced_motion`, which zgx fills from SPI_GETCLIENTAREAANIMATION and updates on WM_SETTINGCHANGE, so no Rust code is needed. The `reduce_motion` state forces reduced motion on, for tests or a future setting of our own.
- Transcript entrances:
  - `lib/arrivals.luau` marks rows as arriving for 600 ms when a change of 3 or fewer rows reaches the same live transcript.
  - Only those rows animate. Rows that were already there, rows scrolled back to, and catch-ups appear as they are.
- Partial → final: a third colour run holds the words that just turned final and eases them from tertiary to the final colour over 150 ms. Then it merges them into the final run. The text never changes, so nothing reflows.
- Theme:
  - `theme.follow(fn)` applies `host.settings.theme` with `ui.theme.set` or `follow_system`.
  - In windows.luau, the overlay window's content is wrapped in a `theme = "recap-dark"` scope.
- Keyboard surfaces, such as the Ctrl K action panel, open instantly.

## Consequences
- The overlay (revised in fix/review):
  - The line shift slides (150 ms). `caption_shift.total` adds up every shift; the captions block moves down by it at once (a plain `y` function) and an inner node eases up by it (`motion.tween` of `-total`), so the sum starts a shift lower and eases to 0. Two nodes do what one binding can't (replay a move from a varying offset).
  - The idle dim eases (400 ms); the undim is instant: the dimmed node is built again (`motion.Rebuilt` on an undim counter), and a new tween starts where its source is.
- Entrances play from the frame after mount.
- A tween's duration is fixed when it is built. `motion.Live(build)` builds a node again as reduced motion changes (a `ui.presence` with no enter/exit, so it lays out in place), and wraps the long-lived tweens: the overlay's slide and dim, Settings' switches, the processing steps' colours. A running animation there stops at once. Wrapping whole windows was tried and dropped: the window's key handlers stopped receiving keys. Short one-off tweens (an entrance, a 150 ms colour run in a row, a chip flash) are not wrapped; they finish within 600 ms.
- Loops don't pause when a window is hidden. zgx doesn't draw a hidden window, and the spec's "pause when minimised" isn't wired separately.
- If zgx ever ticks anime per window, the anime-based forms could come back. Until then, a test that passes headless can still hide content in the app, so check new motion in the real app.
