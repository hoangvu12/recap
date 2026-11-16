# Recap

A Windows desktop meeting notetaker. It shows live captions of the other side of a call in an always-on-top overlay. After Stop it produces a cleaned transcript with speaker names, a summary, next steps and decisions, like Wispr Flow's Notetaker. The look is Raycast for Windows.

Built in **Rust + zgx**: zgx is the Zeron team's GUI framework, with a Luau UI and a Rust host. Personal-use app for one user (Vietnamese, with Indian-accented clients on MS Teams).

## Read first
- `docs/design/DESIGN.md`: scope, flow (§2), screens (§3), architecture (§4), providers (§4.4b), build plan, design system (§9), motion summary (§9.4). The source of truth.
- `docs/design/motion_spec.md`: exact motion and live-text behaviour.
- `docs/design/screens/*.png`: the approved screens (dark) and the clickable flow frames. `docs/design/screens.html` holds every screen as HTML/CSS, including the **icon set** (Icons board) and the **provider logos** with their SVG path data and optical offsets. Take icon and logo geometry from there; don't redraw.
- `docs/design/tokens.json`: design tokens (dark and light).
- `docs/design/raycast_design.md`: the measured Raycast reference behind the tokens.
- `docs/research/research_zgx.md`: what zgx can and can't do, and the workarounds. zgx source: `C:\Users\HP MEDIA\.cargo\git\checkouts\zgx-b5d425ec12da2cd4\24b3587`. The user's own zgx app, the best example: `C:\Users\HP MEDIA\Desktop\nguyenvu\zeron-autonotify\apps\zgx`.
- `docs/research/research_rust_stack.md`: crates and Windows APIs (WASAPI loopback, Ogg/Opus, keyring, tray, hotkey, UI Automation, packaging).
- `docs/research/stt_results.md`: the speech-to-text tests behind the ElevenLabs settings. `reference/` holds the working Python ElevenLabs live client and batch script; port their exact parameters.
- `GLOSSARY.md`: domain words. Use them.

## Hard rules
- **Never commit secrets.** API keys live in Windows Credential Manager (`keyring`) at runtime. For local integration tests, read `ELEVENLABS_API_KEY` from the environment. The dev key file is `C:\Users\HP MEDIA\AppData\Local\Zeron\scratch\2026-11-16-bfca9880\oss_live\elevenlabs\.env`; load it into the env and never copy it into the repo.
- **Never use the private client meeting** (`meeting.ogg` in the scratch folder) in tests, fixtures, commits or screenshots. The only test meeting is `testdata/meeting_tts.wav` (synthetic: Tom, Priya and David; a December 1 launch), with its script in `testdata/meeting_tts_script.txt`. In the script, TTS voice "Mark" = Tom, "Zira" = Priya, "David" = David.
- UI copy: show how it works rather than explain it in text. Avoid "X · Y · Z" strings. Colour is only for speakers, the rec dot, accent (on, links) and errors; provider logos use brand colours.
- Pin zgx at git rev ≥ `24b3587` (earlier revisions crash after 1–2 h).
- Don't touch the user's browser.

## Build and test
- Windows only (MSVC). The laptop has cargo 1.99 and the `zgx` CLI. Heavy builds can go to **rustbox** (a Windows build VM): see the `rustbox` skill. Only Linux SG drives it, so a top-level chat has to delegate there. If that's not available, build on this laptop.
- Rust: `cargo test --workspace`. Luau UI: `zgx test` / `zgx lint`.
- End-to-end regression: replay `testdata/meeting_tts.wav` through the pipeline. Expect Tom, Priya and David named, or "Speaker N", but never a wrong name. The summary must mention the Dec 1 launch, the India soft-launch fallback and the 5 next steps with owners.

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues on `hoangvu12/recap` (private), via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context: `GLOSSARY.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.
