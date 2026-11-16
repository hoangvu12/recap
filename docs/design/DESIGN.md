# Meeting notetaker: design (draft 1, 2026-11-16)

A Windows app, built in Rust with zgx: live captions during a call, then, after the call, a cleaned-up transcript with speaker names, a summary and next steps. The flow follows Wispr Flow's Notetaker, and the look follows Raycast.

Inputs to this doc:
- `ux_study/wispr_ux.md`: Wispr's flow, tested first-hand.
- `ux_study/ui_references.md` and `ux_study/refs/board_*.html`: Otter, Fireflies, Grain, Notion, Granola, Slack.
- `research_zgx.md`: what zgx can and can't do.
- `research_rust_stack.md`: crates and APIs.
- `research_wispr.md`: Wispr internals.
- `oss_live/results.md`: speech-to-text accuracy tests.
- `raycast_capture/`: being captured on the PC. It will become the design-system section.

---

## 1. What it does (v1 scope)

| In v1 | Later |
|---|---|
| Start from the hotkey, the tray, or a "Teams is using your mic. Start notes?" toast | Auto-start, calendar sync |
| Caption overlay: always on top, 1–3 big lines, hidden from screen share | Vietnamese line under each caption |
| Notes window with Notes / Transcript / Summary tabs and a live transcript | "What did I miss?" and Ask-anything chat |
| Local recording of the mic and the meeting audio | |
| After stop: cleaned transcript, speakers split, Me vs Them | |
| Speaker names from what people say plus a typed attendee list, and a rename dialog | Names read from the Teams/Zoom window (UI Automation) |
| Summary in Wispr's layout (overview, topics, next steps with owners, decisions) and an auto title | Templates / "skills" |
| Meetings list with search, Copy as Markdown, export to .md | Share links |
| Keys kept in Windows Credential Manager | |

Not in scope: bots that join calls, cloud sync, teams, mobile.

---

## 2. User flow (Wispr's, adapted)

```
 idle (tray) ──hotkey / tray / "Teams is using your mic" toast──▶ RECORDING
   RECORDING:  caption overlay (Them, live) + notes window (tabs, live transcript, Stop)
      │ Pause/Resume keeps the same meeting (Wispr: Stop = pause)
      ▼ Stop
   PROCESSING (3 visible steps, about 15–40 s for 1 h):
      1 Transcribing     batch STT on the recording (system channel diarized, mic channel separate)
      2 Naming speakers  Me = mic; others from transcript cues + attendee list (LLM)
      3 Writing summary  title, overview, topics, next steps, decisions (LLM)
      ▼
   DONE: Summary tab opens. Transcript shows named speakers. Rename dialog for "Speaker N"
```

Rules taken from Wispr:
- The live view has no names. It shows only **Them** (meeting audio) and **Me** (mic).
- Names appear only with clear evidence; otherwise "Speaker N" with a ▶ voice clip.
- The title is generated at Stop. The summary is regenerated after a rename.
- If the app or the PC crashes, the recording on disk can be processed again ("Recover meeting").

---

## 3. Screens

1. **Caption overlay**: a frameless, always-on-top frosted bar, about 720×120 px, that you can drag and resize.
   - It shows the last committed line plus the live partial line (greyed). "Them" lines are bright, "Me" lines dimmed or hidden.
   - On hover, a mini toolbar appears: pause, stop, open notes, font −/+, show/hide my lines, pin.
   - It is hidden from screen capture and from the taskbar.
   - Click-through can be toggled with a hotkey (zgx only supports whole-window click-through).
2. **Notes window**: a tall side panel like Wispr's, about 420×900 px, snapped to the right edge.
   - Editable title, date chip, and tabs **Notes / Transcript / Summary**.
   - Bottom bar: Stop/Resume and the timer.
   - Transcript rows: speaker chip, time, text. Live rows follow the bottom, with a "Jump to live" pill when you scroll up.
3. **Processing card**: 3 steps with a spinner and check marks, plus errors with Retry.
4. **Summary view** (Markdown):
   - overview;
   - topics (H3 + bullets);
   - **Next steps** as checkboxes with owner chips and dates;
   - **Decisions**.
   - Copy as Markdown and Export.
5. **Transcript view (after)**:
   - coloured speaker chips and a "3 SPEAKERS • 41:12" header;
   - click a line to play the audio from there;
   - click a speaker chip to rename it inline (Otter style).
6. **Assign speakers dialog** (Wispr style): each speaker with 2 sample quotes, a ▶ clip, and a name box that suggests names from People and attendees. "Save names" relabels everything.
7. **Home / meetings list** (Raycast-style list): Start button with hotkey hint, search, meetings grouped by day with duration and speaker count.
8. **Settings**: keys (ElevenLabs, optional Deepgram / Anthropic), my name, glossary (keyterms), audio devices, caption style, LLM choice, storage folder, hotkeys.
9. **Meeting setup** (optional, before or during): attendee names and meeting-specific terms. These feed keyterms and speaker naming.

The look (Raycast) is section 9, filled in after the PC capture.

---

## 4. Architecture

```
┌──────────────────────────── one process ─────────────────────────────┐
│ UI thread (zgx)                                                      │
│   ScriptHost (Luau UI, hot reload)   windows: overlay, notes, home,  │
│   ▲ host.shared(...)  ▲ host.function(row)  │ host.action/async_action│
│   │ (live state)      │ (rows from SQLite)  ▼ (start/stop/rename/…)   │
│   UI tasks ◀── tokio mpsc (segments, lossless) / zgx latest() (partial, levels)
│   tray-icon, global-hotkey, raw Win32 (capture-exclude, tool window)  │
├──────────────────────────────────────────────────────────────────────┤
│ tokio runtime (own threads)                                          │
│  Capture ×2 (wasapi, dedicated threads)                              │
│   mic (default comms device) ─┐                                      │
│   system (process loopback:   ├─▶ resample 16 k mono (rubato) ─┬─▶ Recorder: Ogg/Opus 2-ch (ch0 mic, ch1 system), fsync 1–5 s
│   everything except our app) ─┘   QPC-aligned, gaps zero-filled └─▶ Live STT: ElevenLabs v3 Live socket(s)
│  Live STT client: partial/commit → segments (+ echo dedup) → live.ndjson + SQLite + UI
│  Post-processor (job queue, resumable): batch STT → merge → dedup → speakers → LLM → summary
│  Meeting detector: wasapi capture sessions + ConsentStore + window titles
│  Storage: rusqlite (WAL, FTS5) + per-meeting folder; keyring for keys
└──────────────────────────────────────────────────────────────────────┘
```

**Why this shape:**
- zgx's recommended pattern is a Luau UI with a Rust host. The user's own zgx app (zeron-autonotify) is built the same way.
- Only Luau gets hot reload, `zgx test`, `zgx inspect`, the Markdown view and the follow-the-bottom virtual list.
- All real work stays in Rust. Luau never holds the whole transcript: rows come from Rust on demand.

**Process and windows:**
- `application_id` gives single-instance and toasts.
- `quit_on_last_window_close(false)` lets the app live in the tray.
- Window positions are saved with `frame_autosave`.

### 4.1 Audio
- **System audio:** WASAPI **process loopback, excluding our own process tree**. It survives switching between headphones and speakers, and we never caption our own clip playback.
  - Including only Teams/Zoom is possible for desktop apps.
  - For Teams in Brave it would still capture every tab. So the default is "all system audio".
- **Mic:** the default communications capture device. Follow device changes and reopen on `AUDCLNT_E_DEVICE_INVALIDATED`.
- **Clocking:** both streams are stamped with QPC time.
  - A 16 kHz mono timeline per stream.
  - Pad with zeros when loopback is silent; resync on discontinuity.
- **Echo** (laptop speakers):
  - always drop mic segments that repeat system text (loqui rule: ±1.75 s, similarity ≥ 0.85);
  - optionally use Windows' built-in echo cancellation when the hardware supports it and the output isn't headphones.
- **Recording:** `meetings/<id>/audio.ogg`.
  - Stereo Opus at 32 kbps, about 15 MB per hour.
  - Survives a crash: tested, a cut-off file still decodes.

### 4.2 Live captions (ElevenLabs Scribe v3 Live)
- Hand-written client on `tokio-tungstenite`. Settings are our tested best:
  - `scribe_v3_live`, `turn_prediction`, `no_verbatim`, `language_code=en`;
  - keyterms (glossary + attendees, ≤ 50).
- Show `partial_transcript` live and replace it with `committed_transcript`.
- Send silence so the server doesn't close after 15 s idle.
- **Reconnect:**
  - Replay the audio since the last commit, with `previous_text` on the first chunk.
  - Back off on `session_time_limit_exceeded` and on network errors.
  - Stop and show a banner on `auth_error` and `quota_exceeded`.
- **One socket for system audio. A second for the mic only if the user wants "Me" captions** (decision D1).
- Deepgram Nova-3 stays as a fallback live engine behind the same `LiveStt` trait (the current Python app's settings).

### 4.3 After stop
1. **Batch STT** (ElevenLabs `scribe_v3`, `timestamps_granularity=word`, `no_verbatim`, keyterms):
   - **System channel alone with `diarize=true`**. ElevenLabs can't diarize in multichannel mode.
   - **Mic channel alone without diarize**: it's all "Me".
   - Both requests run in parallel. A 7.7-min test took 11.5 s.
   - Tested on our real meeting: **8.3%** vs Whisper, about Wispr-refined level, and better than Deepgram's file transcript (9.5%).
2. **Merge** both word lists by time into turns. Drop mic turns that repeat system text (echo). Store as `refined.ndjson` plus SQLite rows. The refined transcript replaces the live one.
3. **Speaker naming** (LLM, structured JSON):
   - Input: turns with ids S1..Sn, the attendee list, my name, the summary of who-says-what.
   - Output: `{speaker_id: {name|null, evidence quote, confidence}}`.
   - Rules: name only from explicit evidence (addressed by name, self-introduction, "thanks, Priya" right before a turn), never from guesses.
   - Then apply matches against known people. Unnamed speakers stay "Speaker N".
   - v2 adds votes from the meeting app's highlighted-speaker tile (UI Automation), as Wispr does.
4. **Summary** (LLM, structured JSON → Markdown):
   - Input: the named transcript plus my notes.
   - Output: `{title, overview, topics[{heading, bullets}], next_steps[{owner, text, due}], decisions[]}`.
   - My notes steer the emphasis, as in Wispr. Rendered with `ui.markdown_view`.
   - A 1 h meeting is about 15–20k tokens, so one call is enough.

### 4.4 LLM backend (trait `Llm`)
- **Default for personal use:** the installed `claude -p` with the user's own Claude login. No key, no extra cost.
  - `--output-format json --json-schema … --tools "" --no-session-persistence`, with the transcript on stdin.
  - Run from an empty temp folder.
  - **Remove `ANTHROPIC_API_KEY` from the child's environment**, or the call is billed to the API.
  - Allowed for a personal tool; not allowed in a shipped app.
- **Option:** Anthropic API key via `reqwest`. About $0.06 per meeting-hour on Sonnet 5.5, under $0.01 on Haiku 5.5.
- **Not usable now:** Codex CLI (login expired) and Gemini CLI (personal accounts blocked). Antigravity `agy` works but ignores stdin.

### 4.4b Providers (decided 2026-11-16)
Every engine is a **provider** = a kind + its config. The app picks one **main** and one **backup** for each job: Live captions, Transcript (after stop), Summary and names.
- **Speech-to-text: adapters in Rust code** (ElevenLabs, Deepgram first). Each one implements `LiveStt` and/or `BatchStt` and declares its **capabilities**: `live`, `batch`, `diarize`, `keyterms`, `word_timestamps`. The UI reads them: capability chips on each row, and the pickers only list providers that can do that job. Adding a vendor = one new adapter file + a registry entry; no UI work.
- **AI: config-driven**, three kinds behind the `Llm` trait (text in, JSON out):
  - `cli`: a command template with `{prompt}`, `{schema}`, `{transcript_file}` tokens; transcript via stdin or file. Presets: Claude Code, Codex, Gemini CLI.
  - `openai_compatible`: base URL + key + model. Presets: OpenAI, OpenRouter, Groq, DeepSeek, Mistral, Ollama (local).
  - native: Anthropic (Gemini later).
  - If a provider can't enforce a JSON schema: validate the reply, retry once with the error.
- Keys go in Credential Manager; every provider has a **Test** button.
- Logos: marks from Simple Icons / LobeHub icons in their brand colours (Claude #D97757, Deepgram #13EF93, Groq #F55036, DeepSeek #4D6BFE, OpenRouter #94A3B8, Gemini and Mistral gradients) on a 10% tile. Black brands (OpenAI, Anthropic, ElevenLabs, Ollama, Codex) use `text.primary`, so they flip with the theme.
- Screens: Settings → Providers, Add provider (picker), Add provider forms (API, custom command).

### 4.5 Meeting detection (v1: suggest, don't auto-start)
- Poll active capture sessions every 1 s (wasapi) and cross-check the ConsentStore registry key (HKCU) to see which program uses the mic.
- Window titles/classes identify Teams, Zoom or Meet.
- When a known meeting app starts using the mic: a toast "Teams call detected. Start notes?" with a Start button.
- When the mic is released for more than 60 s: a toast offering to stop.

### 4.6 Storage
`%LOCALAPPDATA%\<App>\`
- `app.db`: SQLite, WAL, FTS5.
  - Tables: `meetings`, `segments` (live|refined), `speakers`, `people`, `notes` (my notes, summary JSON/MD), `keyterms`, `speaker_observations` (v2).
- `meetings/<id>/`:
  - `audio.ogg`;
  - `live.ndjson`, `refined.ndjson` (append-only; the DB can be rebuilt from them);
  - `stt_system.json`, `stt_mic.json` (raw responses);
  - `summary.md`.
- Keys are stored in Windows Credential Manager (`keyring`). Rotate the ElevenLabs key that was pasted in chat.
- Retention setting: keep audio for N days (default: keep).

### 4.7 Crates (pinned)
- Audio and STT: `wasapi` 0.27, `rubato` 5.1, `windows` =0.62.2, `opus` 0.4 (bundled; needs VS "C++ CMake tools") + `ogg` 0.9, `tokio` 1.54, `tokio-tungstenite` 0.30 (rustls native roots), `reqwest` 0.13.5.
- Storage and platform: `rusqlite` 0.43 (bundled), `keyring` 4.2, `tray-icon`, `global-hotkey`, `rodio` (voice clips), `serde`/`serde_json`, `directories` 6.
- UI: zgx pinned at git rev ≥ `24b3587`. Earlier revisions leak a Windows graphics handle and crash after 1–2 h. Copy zgx's `[patch.crates-io]` block.
- Packaging: `cargo-packager` 0.11 (per-user NSIS installer, no admin prompt). No code signing for personal use.

### 4.8 What zgx lacks, and our workaround
| Gap | Workaround |
|---|---|
| No HWND accessor | After the first frame, `EnumThreadWindows` + class `"Window Class"` + unique title. Or upstream a small accessor to zgx (its own rule: "fix zgx, not around it") |
| Hide overlay from screen share / taskbar | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`, `WS_EX_TOOLWINDOW` (raw Win32) |
| Tray, global hotkey, autostart | `tray-icon`, `global-hotkey`, HKCU Run key |
| Audio playback | `rodio` on its own thread |
| Rich-text notes | Plain `ui.textarea` with Markdown highlighting (fine for v1) |
| Per-region click-through | Toggle whole-window click-through with a hotkey or a hover handle |

---

## 5. Costs per meeting-hour (list prices, Nov 2026)

| Part | Option | $/h |
|---|---|---|
| Live captions | Them only: v3 Live $0.22 + keyterms $0.08 | **0.30** |
| | Me + Them (2 sockets) | 0.60 |
| After stop | ElevenLabs batch: system (diarized) + mic, $0.20 + $0.05 keyterms each | **0.50** |
| | Deepgram pre-recorded, multichannel + diarize (from the $196 free credit) | ~0.6, $0 out of pocket |
| LLM (naming + summary + title) | `claude -p` on the user's subscription | **0** |
| | Anthropic API: Sonnet 5.5 / Haiku 5.5 | ~0.06 / <0.01 |
| **Recommended total** | Them-only live + ElevenLabs batch + claude -p | **≈ $0.80/h** |

**ElevenLabs Starter ($6/mo) budget:** it lists 27 realtime hours or 30 batch hours. Both equal about $6 of usage, so it is **likely one shared credit pool** (check the usage page).
- At $0.80/h that is about **7–8 meeting-hours a month**.
- With Deepgram for the after-stop step, ElevenLabs only pays for live captions ($0.30/h): about **20 meeting-hours a month**.
- More hours need the next ElevenLabs tier.

---

## 6. Decisions for you

| # | Question | My recommendation |
|---|---|---|
| D1 | Live captions for **Them only**, or Me too? | Them only: half the live cost, and you don't need captions of yourself. Your words still get transcribed after stop |
| D2 | After-stop transcription: **ElevenLabs** (better, 8.3%) or **Deepgram** (free credit, 9.5%)? | ElevenLabs if your monthly meeting hours fit the plan, else Deepgram. Make it a setting |
| D3 | LLM: **`claude -p` on your Claude login**, or an **API key**? | `claude -p` (no key, no cost). API key as an optional setting |
| D4 | About how many meeting hours a month? | Decides the ElevenLabs plan (Starter ≈ 7–20 h depending on D2) |
| D5 | Usually headphones or laptop speakers? | If speakers: echo dedup is a must in v1 (it's planned anyway) |
| D6 | Meeting detection in v1: suggest-only toast, or manual only? | Suggest-only toast (cheap to build, no surprise recordings) |
| D7 | Names from the Teams window (UI Automation) in v1 or later? | Later: fragile, untested on a live call. v1 uses transcript cues + attendee list + rename |
| D8 | Theme: Raycast **dark by default**, light optional? Overlay always dark? | Yes and yes |
| D9 | App name? | Your call (Wispr calls it Notetaker; ours could be e.g. "Hear", "Recap", "Sidecar") |

---

## 7. Build plan (after the design phase)

| Milestone | Deliverable | Done when |
|---|---|---|
| M0 Skeleton | zgx app, tray, hotkey, overlay + notes windows on fake data, design tokens | Overlay is on top, hidden in a Teams screen-share test |
| M1 Live | System loopback + mic capture, Ogg recorder, ElevenLabs live → overlay + transcript, reconnect | Parity with the Python prototype on `meeting.ogg` replay (≈9.5%) |
| M2 After stop | Batch STT ×2, merge, echo dedup, refined transcript, crash recovery | 8.3% on the test meeting; a killed app recovers |
| M3 AI | `claude -p` naming + summary + title, summary view, Copy/Export Markdown | TTS test meeting: Tom/Priya/David named or "Speaker N" with no wrong names |
| M4 Library | Meetings list + search, rename dialog with clips, settings, keys in Credential Manager | |
| M5 Polish | Detection toast, installer, autostart | Daily use for a week |
| Later | UI Automation names, Vietnamese line, "What did I miss?", Ask | |

**Test assets:**
- `meeting.ogg` (private; scored only through ElevenLabs/Deepgram, never uploaded elsewhere);
- `ux_study/meeting_tts.wav` (synthetic 3-speaker standup with known names, decisions and owners): use it as the end-to-end regression test.

---

## 8. Risks
1. **zgx maturity:** Windows is a secondary target, the API changes fast, and the long-session crash is fixed only in recent revisions.
   - Mitigation: pin the revision; run a 3 h soak test with a live transcript in M1.
2. **ElevenLabs quota:** a meeting cut off mid-way by `quota_exceeded`.
   - Mitigation: show remaining hours; fall back to Deepgram live automatically.
3. **Process loopback timestamps** (unverified) and silent loopback gaps.
   - Mitigation: verify in M1; zero-fill from QPC.
4. **`claude -p` changes or usage limits.**
   - Mitigation: keep the `Llm` trait; the API key path is a 50-line swap.
5. **Speaker naming errors:** be conservative. A wrong name is worse than "Speaker 3".

---

## 9. Design system (Raycast-based)
Measured from Raycast for Windows 2.12.1 on the PC. Values were sampled from pixels at 100% scale and cross-checked against Raycast's own CSS. Full reference and screenshots: `ux_study/raycast_capture/raycast_design.md` and `01_…13_*.png`.

### 9.1 Principles (what we copy)
1. **One hue, many opacities.** Every surface, border and text colour is the foreground (white in dark, black in light) at 5/10/20/40/60%. A theme is just 5 inputs.
2. **No boxes unless needed.** Search fields and footers are bare text on the window material. Regions are split by 1 px hairlines at fg 10%.
3. **Selection is a soft wash:** fg 10%, 8 px radius, inset 8 px. There is no accent bar. Accent colour is only for "on" states, links and the record dot.
4. **Hierarchy from weight and opacity, not size:** 13 px for almost everything, 11 px for headers and hints, 18 px for the one big input.
5. **Keyboard-first:** quiet outlined keycaps in every footer and menu, and the primary action is always the bright label next to ↵.
6. **Heavy-tint acrylic** (about 90% tint), DWM 8 px corners, and the Windows system accent. It feels native.
7. **Calm motion:** 100–200 ms with ease-out-expo; the list selection moves instantly.

### 9.2 Tokens
| Token | Dark | Light |
|---|---|---|
| `bg.window` (acrylic, 90% tint) | `#1D1D1D`, renders ≈ `#272727` | `#FFFFFF`, renders ≈ `#F2F2F2` |
| `bg.window.solid` (transparency off) | `#262626` | `#FFFFFF` |
| `bg.raised` (popover, action panel) | `#363636` + border `#5E5E5E` | `#F2F2F2` + border `#C2C2C2` |
| `bg.card` (settings groups) | `#232323` | `#F8F8F8` |
| `fill.hover / selected / control` | fg 5 / 10 / 10% | same |
| `stroke.hairline / field / focus` | fg 10 / 20 / 50% | same |
| `text.primary / secondary / tertiary / quaternary` | fg 100 / 60 / 40 / 20% (`#FFF`/`#A8A8A8`/`#7E7E7E`/`#525252`) | (`#000`/`#616161`/`#919191`/`#C1C1C1`) |
| `accent` | Windows system accent (fallback `#4FA3F8`) | system accent (fallback `#138AF2`) |
| `rec` (recording dot, Stop) | `#FF6363` (Raycast brand red) | `#B12424` |
| Hues (speaker chips, tags) | red `#FF6363`, orange `#FF9217`, yellow `#FFC531`, green `#59D499`, blue `#56C2FF`, purple `#A485FF`, magenta `#CF2F98` | `#B12424`, `#C75D07`, `#F8A300`, `#006B4F`, `#138AF2`, `#6A3DEC`, `#9A1B6E` |
| Font | **Inter Variable** (bundle it, as Raycast does); JetBrains Mono for code/time | |
| Type scale | 10 (keycaps), 11/500 (section headers, labels), **13/400–500 (body)**, 16, 18 (search/title input), 24/600 (hero) | |
| Spacing | 2, 4, 6, 8, 12, 16, 24, 32, 40, 48 | |
| Radius | window 8 · row 8 · control/menu item 6 · keycap 4 · chip/switch full | |
| Sizes | header 64 · list row 38 · menu item 32 · field 32 (+2 border) · footer 42 · settings row 47/58 · switch 28×16 · keycap 18 h · icons 16/22 | |
| Shadow (popovers) | `0 8 10 -6 #0000001A, 0 20 25 -5 #00000033` + 1 px border | |
| Motion | hover 100 ms in / 200 ms out; default 150 ms ease-out; ease-out-expo `cubic-bezier(.16,1,.3,1)`; switch overshoot `cubic-bezier(.16,-.12,.29,1.31)` 366 ms | |

The zgx mapping happens in M0. These become the app's `ui.theme` tokens for dark and light with `follow_system`. Glass comes from `Glass::…` (Acrylic/HUD for popovers, `.plain()` for the overlay). Inter ships with the app.

### 9.3 Components for our screens
| Component | Spec (from Raycast) | Used in |
|---|---|---|
| **Window shell** | 8 px corners, acrylic, header 64 / hairline / content / hairline / footer 42 | Home, Notes |
| **Footer action bar** | left: context (icon + meeting name, 13 secondary); right: primary action label + ↵ keycap │ "Actions" + `Ctrl K` | Notes ("Stop ↵"), Home ("Start notes ↵") |
| **Action panel** (Ctrl+K) | 350×280 popover above the footer, right-aligned, 32 px items with 16 px icon + keycaps, "Search for actions…" at the bottom | Rename speakers, Copy Markdown, Export, Regenerate summary, Delete |
| **List row** | 38 px, 22 px icon, title 13/500 + subtitle 13/400 secondary, right accessory (duration, "3 speakers"), 8 px inset selection | Meetings list, People |
| **Section header** | 11/500 secondary, 32 px block, no rule | "Today", "Yesterday", "Next steps" |
| **Search header** | bare 18 px text, dropdown accessory on the right (197×34, 6 px radius, fg 20% border) | Home search + filter (All / With summary) |
| **List + detail** | list column 284 px, 1 px divider, detail pane with an "Information" metadata block (33 px rows, label left / value right) | Home: meeting list + preview of the summary |
| **Tabs** | text tabs 13/500. Selected = primary + 2 px accent underline; others secondary | Notes / Transcript / Summary |
| **Transcript row** | speaker chip (hue at 10% fill, hue text, full radius, 11/500) + time 11 tertiary mono, then text 13/400 primary; partial text in tertiary | Live + refined transcript |
| **Caption bar** (overlay) | dark acrylic always, pill-ish 8 px radius, 24 px side padding (HUD spec), 18–24 px text (user-adjustable), the last line primary and the partial tertiary, a 6 px `rec` dot + timer on the left, hover toolbar of 28 px icon buttons | Overlay |
| **Field** | 32 px, 6 px radius, fg 20% border, dark fill fg 5%; focus = fg 50% border, no glow | Settings, speaker name, attendees |
| **Combobox** | field + popover (196 px+, 32 px items, "Search…" at the bottom) | Assign speakers |
| **Settings card** | grouped cards, 8 px radius, 47/58 px rows, label left / control right, section title 13/500 outside | Settings |
| **Switch / keycap / badge** | 28×16 accent switch; outlined 18 px keycap; pill badge at hue 10% fill | Settings, footers |
| **Progress steps** | 3 rows of 32 px: spinner → ✓ (green) → ✗ + Retry (red); 13 px labels | Processing card |
| **Toast / HUD** | HUD pill 46 px, 22 px icon, 24 px padding, max 500 w; in-window toast 44 px in the footer area | "Copied", "Summary ready", errors |
| **Checkbox item** | 16 px rounded-4 box, fg 20% border, accent when checked; owner chip + due date as tertiary accessory | Next steps |

### 9.4 Motion and live text
Full spec: `ux_study/motion_spec.md` (Wispr Flow's code read statically, then adapted to these tokens). The design canvas can't show motion, so that file is the source of truth. The short version:
- **Tokens:** instant 0 · fast 100/200 ms · base 150 · enter 200 · reveal 240, all `ease-out-expo` (Raycast); entrance offset 4 px. Animate only opacity, colour and transform, never layout or blur.
- **Live text:** partial words in `text.tertiary` (overlay fg 55%) right after the final words, updated with no animation. When they become final they fade to `text.primary` over 150 ms. No cursor, shimmer or typewriter.
- **Follow the bottom:** instant. Scrolling up unpins and shows the "↓ Live" / "↓ N new" pill (150 ms fade + 4 px rise). New rows: 200 ms fade + 4 px rise, and none when more than 3 land at once.
- **Overlay:** lines bottom-aligned; a wrap or new turn shifts the block up one line (150 ms translate); the hover toolbar appears after a 150 ms delay; idle for 8 s → text dims.
- **Recording:** the rec dot breathes (opacity, 2 s); optional 3 level bars in the notes footer only.
- **Processing:** 3 step rows; spinner → ✓ crossfade 150 ms; hold 400 ms on all ✓, then the summary sections fade in (240 ms, 40 ms stagger, ≤ 500 ms total). After a rename, only changed paragraphs get a 600 ms wash.
- **Reduced motion** (Windows "Animation effects" off): no transforms, no loops, fades become 100 ms or instant. Loops pause when the window is hidden.
- **M0 check:** zgx may not support two colour runs in one text block ("no inline nodes"). If not, the partial goes in a second text node that wraps after the final one (see the spec §2.1).

Next design step: build these screens in a design tool from these tokens, following the Wispr flow (section 2) and the screens list (section 3).
