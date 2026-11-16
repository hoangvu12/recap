# 0015 Renames relabel by id; playback on its own thread

## Context

Ticket #12: rename a speaker inline or in the Name speakers dialog, keep it, update every view, write the summary again with only the changed paragraphs highlighted; play a line from where it starts and a clip of each voice.

## Decision

- **A rename changes the meeting's speakers, not the transcript.** Refined segments keep their speaker ids (`me`, `S1`..); every view (rows, header chips, Home, the AI prompts) labels from `MeetingInfo.speakers`. `Store::rename_speaker` writes info.json and the speakers table; every name a meeting gives goes into `people` (case-insensitive, rebuilt from the folders), which feeds the suggestions. Me is locked: it is Settings' name.
- **After a rename, only Writing summary runs again** (`processing::redo(.., [WritingSummary])`), and only for a meeting that has a summary. Renamed while processing: it runs once processing ends. Naming speakers is not rerun, so the AI never overrides a name the user gave or left out.
- **What changed is found in Luau.** The Summary tab compares the new summary with the one it showed for the same meeting, paragraph by paragraph (overview, headings, bullets, next steps with owner and due, decisions). Changed ones get the 600 ms wash; nothing fades in again. A summary written again doesn't switch tabs; only a newly arrived one opens the Summary tab.
- **Playback is a thread with a std channel** (`recap::playback::Player`). It decodes `audio.ogg` (`Recording::read`, the last one cached), cuts the clip and plays it through an `Output` (rodio 0.23, playback feature only, the device opened at the first clip). Events come back on a tokio channel the UI thread awaits. A line plays Me and Them mixed from its start to the end; a voice's clip is its longest turns up to 8 s, its side only, 300 ms apart.
- **The dialog is a window** (`speakers`, 480×620, frameless) as in the flow frame, not a modal over the 420 px Notes panel.

## Consequences

- Renames are cheap and can't desync the transcript, but anything that wants names must read them from the speakers (never from segment text).
- A rename costs one AI call when a summary exists.
- Rows and clips need `audio.ogg`; after audio retention deletes it, playing says "This meeting's recording is gone."
