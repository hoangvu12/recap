# 6. Settings: one typed struct in settings.json; the storage folder moves only the meetings

Status: accepted (ticket #16)

## Context

Settings (screens/8) hold the name, theme, caption style, audio sources,
glossary, hotkeys, storage folder and audio retention. Later tickets read
them: the overlay (#8), capture (#7), the speech engines (#6, #14), the
pipeline (#11), the tray and hotkeys (#15), the theme (#17). The storage
folder can move where meetings live, so settings can't live inside it.
ADR 0004 left an unused `settings` table in `app.db`, which is lost with
the database.

## Decision

- **`recap_storage::Settings` is the one typed struct.** It is saved whole
  to `settings.json` in Recap's home (`RECAP_DATA`, else
  `%LOCALAPPDATA%\Recap`), beside `providers.json`. Every field has a
  default, so older or hand-edited files load. `normalized()` brings values
  into range: caption size 14 to 40 px, 1 to 3 lines, a valid glossary, and
  hotkeys in one spelling with no repeats. A file that isn't JSON is an
  error, not a silent reset. The `settings` and `keyterms` tables stay
  unused.
- **The glossary is the keyterms.** A term is trimmed, at most 20
  characters (characters, not bytes), unique ignoring case, and there are
  at most 50. `keyterms()`, `keyterms_with(attendees)` and `live_options()`
  give the engines a list that is always within 50 × 20.
- **The storage folder moves only `meetings/` and `app.db`.** "" means the
  home folder. Changing it opens a `Store` there right away; Home then lists
  the meetings in that folder. Nothing is copied. Point it at a folder that
  already has meetings to use them.
- **Audio retention** (`keep_audio_days`, 0 keeps everything) deletes
  `audio.ogg` of meetings that are `done` and older than N days, at start
  and when the setting or folder changes. Transcripts, summaries and
  `stt_*.json` stay. Unfinished meetings keep their audio for Recover.
- **On the app side, `SettingsHost`** publishes the shared `settings`
  state, saves each change at once, and passes it on through `on_change`
  (UI thread) and `subscribe()` (a tokio `watch`, for engine tasks).

## Consequences

- Behaviour that belongs to other tickets only reads the struct: overlay
  size, lines, partial text and screen-share hiding (#8); mic and
  meeting-audio source (#7); hotkeys, call toast and autostart (#15);
  theme (#17).
- Retention runs at start and on change, not on a timer. A run of several
  days without a restart prunes late. That's fine for daily use.
