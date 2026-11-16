# 0009 Live session: what is saved, echo dedup, pause markers

## Context

Ticket #7 joins capture, the store, the Live captions job and the UI. Three choices needed making: when a live final is saved, how a mic echo of Them is dropped when Me is captioned too, and how Pause/Resume stays one meeting.

## Decision

- **Saving.** `recap::session` (headless, no zgx) creates the meeting at Start (status recording), records `audio.ogg` into its folder, and appends a final to `live.ndjson` and the index when the engine *commits* it (`EngineEvent::Committed`). At Stop it saves the duration and status `processing` (ready for #9), writes `pauses.json`, then sends `Saved`. Home doesn't list the live meeting until it is saved.
- **Echo.** The rule is `recap_engine::echo::is_echo`: a Me segment within ±1.75 s of Them with a word similarity of at least 0.85 is an echo. Similarity is the in-order word overlap (LCS) over Me's length. A Me segment under 5 words must match a whole Them segment (Dice), so "Yes." or "One more week?" said back isn't taken as an echo. Live, only Me *finals* are judged, because partials are too short. A Me final is shown at once and committed only once Them's captions have passed its end by 1.75 s, or Them's audio has passed it by 4.75 s. If it turns out to be an echo it is taken back (`EngineEvent::Dropped`): the row keeps its place with no words, and a final row the UI already read is sent again under `transcript.revised`. With "Caption my own speech" off, nothing is judged.
- **Pauses.** The engine keeps one meeting. On Resume (or Stop while paused) it emits `EngineEvent::Pause(PauseMark { at_ms, paused_ms, resumed_ms })` and adds a marker row (`Row.pause`), shown as "15:54 – 15:58". Pauses go to `MeetingResult.pauses` and `pauses.json`.

## Consequences

- Rows stay append-only with stable indexes. A dropped echo leaves a blank row, which the UI draws as nothing.
- The Me finals saved to `live.ndjson` arrive up to about 5 s late. Their times are still meeting time.
- The after-Stop pipeline (#9) should reuse `echo::is_echo` on the batch transcripts. A stored meeting's view doesn't read `pauses.json` yet.
