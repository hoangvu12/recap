# 4. Storage: meeting folders are the truth, app.db is an index

Status: accepted (ticket #3)

## Context

A meeting must survive a crash, and the database must be rebuildable
(DESIGN §4.6). Later tickets write to a meeting from different places: the
live session (segments), the pipeline (refined segments, speakers, summary)
and Home's actions (rename, delete, recover, search, export).

## Decision

- **`crates/storage` (recap-storage) is one deep module behind `Store`.**
  Writers: `create`, `append_live`, `set_refined`, `update(id, |info| …)`,
  `set_my_notes`, `set_summary`, `delete`. Readers: `list`, `meeting`,
  `segments`/`transcript`, `search`. `rebuild` re-reads every folder. No zgx,
  no engine logic; it uses only the engine's `Source`.
- **Each write goes to the folder first, then to SQLite** in one short
  transaction. `meetings/<id>/` holds `info.json` (title, start, duration,
  status, speakers), `live.ndjson` (appended), `refined.ndjson` (rewritten
  whole), `my_notes.md`, `summary.json` + `summary.md`. JSON files are
  written to a temporary file and renamed into place.
- **`app.db` (WAL, FTS5) is an index of the folders.** If it is missing,
  `Store::open` rebuilds it. A live file cut off mid-line by a crash rebuilds
  up to the cut. Search is one FTS5 table of title and speaker names, segments
  and the summary, with every word matched as a prefix.
- **Ids are the start time in UTC, `YYYYMMDD-HHMMSS`** (`-2`, `-3` if two
  meetings start in the same second): folders sort by time and say when
  they're from.
- **Status** is `recording` → `processing` → `done`. A meeting that isn't
  `done` and isn't live is unfinished, and Home offers Recover.
- `Store` is `Clone + Send + Sync` (one connection behind a mutex), so the
  live session and the pipeline can write from tokio tasks.

## Consequences

- The app's own tables (`people`, `keyterms`, `providers`, `settings`) exist
  only in the database. A rebuild keeps them, but deleting `app.db` loses
  them. They're small, and Settings can rewrite them; give them files too if
  that turns out to matter.
- *Amended (final review):* `people` mattered (the rename suggestions grow
  from every meeting, also names a meeting no longer gives). They are kept in
  `people.json` too, in Recap's home beside settings.json
  (`Store::open_with_people`; `Store::open` keeps it in the store's root).
  Every rename that names someone rewrites it if it changed, and a rebuild
  reads it back before indexing the folders, so losing `app.db` loses no one.
  `keyterms` and `settings` stay unused (settings.json holds both).
- Search with several words needs every word to appear in one row (one
  segment, or the title and names). That's enough for #13 to start from.
- Home's day groups are computed in Luau (local time). Luau's `os.time(table)`
  reads a table as UTC; `lib/format.luau` corrects for that.
