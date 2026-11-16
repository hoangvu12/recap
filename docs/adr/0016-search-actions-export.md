# 0016: Search per word across a meeting; the action panel; Markdown export

## Context

Home search must find "the meeting where we said Dec 1" by title, people and transcript words (#13). The FTS5 index (ADR 0004) holds one row per title+names, per segment and for the summary, so a plain `MATCH "priya"* "caching"*` needs both words in one row and misses a meeting where Priya is a speaker and someone said "caching". The action panel (Ctrl K, screens/9) needs Copy as Markdown, Export .md, Name speakers, Regenerate summary, Show recording in folder and Delete in Home and the Notes window.

## Decision

- **Search:** each query word is a quoted prefix matched on its own; a meeting must hold every word somewhere; its score is the sum of each word's best bm25 rank. Words of punctuation only are dropped. "Dec 1" finds "December 1st" (prefixes). Home shows the host's hits plus rows whose own text matches (e.g. "Unfinished meeting"), grouped by day as before.
- **Recover** shows only for a meeting that is not done *and* still has `audio.ogg` (`Store::has_recording`), matching the glossary. Unfinished meetings with nothing recorded show their length; Enter still opens them.
- **Markdown** is built in Rust (`recap::export::meeting_markdown`): `# title`, Date/Duration/Speakers lines, `## Summary` (the stored summary Markdown), `## My notes`, `## Transcript` with `**Speaker** (m:ss): text`. The date string comes from the UI (local time lives in Luau). Clipboard and the save dialog are the UI's (`ui.clipboard.write`, `ui.save_file` + `ui.write_file`), so `zgx test`'s fake desktop covers them.
- **Action panel** is one Luau component (`components/action_panel.luau`) per window; `state.actions_open` names the window ("home"/"notes"), since both windows share state. It opens only on a meeting: Home's selected one, or the Notes window's once stopped and stored. Each action's keys also work with the panel closed. The Notes footer's primary becomes Copy Markdown ↵ once processing is done.
- **Name speakers** invokes the host action `open_name_speakers(meeting_id)`, which ticket #12 registers. The label follows the ticket ("Name speakers…"), not screens/9's older "Assign speakers…".
- **Delete** asks in an in-window dialog (Delete ↵ / Cancel Esc), then `delete_meeting(id)`: refused while live or processing; the folder goes first, then the rows; the Notes window closes if it showed it.

## Consequences

- A long query costs one FTS query per word; fine at personal scale.
- Until #12 merges, choosing Name speakers is a script error ("no host action named open_name_speakers").
