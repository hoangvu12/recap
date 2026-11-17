# Glossary

| Term | Meaning | Avoid |
|---|---|---|
| **Meeting** | One recording session from Start to Stop, including pauses. It has a folder on disk and a row in the database. | call, session (except for a provider's websocket session) |
| **Me / Them** | The two audio sources. **Me** is the mic. **Them** is the meeting audio (system loopback), i.e. the people you're meeting with. | local/remote, user/other |
| **Live transcript** | Segments produced during the meeting by the live STT provider. Replaced by the refined transcript after Stop. | |
| **Segment** | One unit of transcript text with source (Me/Them), start/end times and text. | utterance, chunk |
| **Partial** | Live text the engine may still revise. Shown in `text.tertiary`. | interim, draft |
| **Final** | Live text the engine has committed. Shown in `text.primary`. | committed (except in ElevenLabs' own message names) |
| **Refined transcript** | The after-Stop transcript from batch STT: system channel diarized, mic channel separate, merged and de-echoed. | second pass |
| **Speaker** | A diarized voice in the refined transcript: `S1..Sn`. It has a name or stays "Speaker N". Me is always the user's name. | participant |
| **Speaker naming** | The LLM step that names speakers only from explicit evidence (addressed by name, self-introduction). | identification |
| **Summary** | The structured result: title, overview, topics, next steps (owner, text, due) and decisions, rendered as Markdown. | notes (the user's own typed notes are **My notes**) |
| **My notes** | What the user types in the Notes tab during the meeting. Steers the summary. | |
| **Processing** | The after-Stop pipeline: Transcribing → Naming speakers → Writing summary. Each is a **step** with pending/running/done/error. | post-processing |
| **Provider** | An engine for one or more **jobs**: a kind + its config (key, model, command). | backend, vendor (in code) |
| **Job** | What a provider is used for: **Live captions**, **Transcript**, **Summary and names**. Each job has a main and a backup provider. | task |
| **Capability** | What an STT provider can do: `live`, `batch`, `diarize`, `keyterms`, `word_timestamps`. Pickers only list providers that can do the job. | feature |
| **Keyterms** | Words the STT should spell right: glossary + attendees, max 50 × 20 chars. | hotwords, boost words |
| **Overlay** | The always-on-top caption bar. Always dark, hidden from screen capture. | HUD, subtitle window |
| **Notes window** | The side panel with the Notes / Transcript / Summary tabs. | main window |
| **Home** | The meetings list window (Raycast list + detail). | library |
| **Echo dedup** | Dropping mic segments that are the meeting audio heard back: by level (the mic was barely above the speaker bleed while Them spoke, ADR 0021) or by text (they repeat system text, ±1.75 s, similarity ≥ 0.85). | |
| **Bleed** | Meeting audio picked up by the mic from the speakers. Measured as the mic's level relative to Them's. | crosstalk |
| **Meeting language** | The language setting sent to the STT providers. **Auto** lets the provider detect it, including switches mid-call (ADR 0022). | locale |
| **Click-through** | The overlay letting the pointer pass to the window under it. Reset off for every new meeting (ADR 0020). | ghost mode |
| **Recover** | Reprocessing a meeting whose recording exists but which never finished processing (crash, killed app). | |
