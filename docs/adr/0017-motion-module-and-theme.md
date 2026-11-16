# 0017: One motion module; the theme follows Settings; the overlay is scoped dark

## Context
motion_spec §2 gives motion tokens and a reduced-motion rule (Windows "Animation effects" off: no transforms, fades of 150 ms or more become 100 ms, everything else instant, loops still). Screens animated ad hoc with `anime.animate`/`ui.loop`. Settings stores `theme` (system/dark/light) but nothing applied it. The overlay must stay dark.

## Decision
- `ui/lib/motion.luau` holds the tokens and is the only way the UI animates: `enter`, `fade`, `pop`, `animate`, `tween`, `color`, `exit`, `Looping`. It applies the reduced-motion rule itself (`motion.reduce`), reading `ui.prefers.reduced_motion`. zgx fills that from SPI_GETCLIENTAREAANIMATION and updates it on WM_SETTINGCHANGE, so no Rust code is needed. The `reduce_motion` state forces it on, for tests or a future setting of our own.
- Entrances are decided at mount (on_mount). Tweens and presence exits read reduced motion when they are built. Loops swap live (`Looping`).
- Transcript entrances: `lib/arrivals.luau` marks the rows of a change of 3 or fewer rows to the same transcript as arriving for 600 ms. Only those rows animate, so rows that were already there, rows scrolled back to, and catch-ups appear as they are.
- Partial → final: a third colour run holds the words that just turned final and eases them from tertiary to the final colour over 150 ms, then merges them into the final run. The text never changes, so nothing reflows.
- Theme: `theme.follow(fn)` applies `host.settings.theme` (`ui.theme.set` or `follow_system`). The overlay window's content is wrapped in a `theme = "recap-dark"` scope in windows.luau.
- Keyboard surfaces (the Ctrl K action panel) open instantly.

## Consequences
- Screens #8 and #12 own (the overlay, the speakers dialog, inline rename) should switch to `lib/motion` (see LEARNINGS).
- A tween built while motion is allowed keeps its duration if the user turns animations off mid-session. Entrances and loops follow at once.
- Loops don't pause when a window is hidden: zgx doesn't draw a hidden window, and the spec's "pause when minimised" isn't wired separately.
