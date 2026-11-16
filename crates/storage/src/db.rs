//! `app.db`: the index of the meeting folders, plus the app's own tables
//! (people, keyterms, providers, settings) that later tickets fill.

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{
    Meeting, MeetingId, MeetingInfo, Result, Segment, Speaker, Status, Summary, TranscriptKind,
    files::Folder,
};

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meetings (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    started_ms  INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    status      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS meetings_by_start ON meetings (started_ms DESC);
CREATE TABLE IF NOT EXISTS segments (
    meeting_id TEXT NOT NULL REFERENCES meetings (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('live', 'refined')),
    seq        INTEGER NOT NULL,
    source     TEXT NOT NULL,
    speaker    TEXT,
    start_ms   INTEGER NOT NULL,
    end_ms     INTEGER NOT NULL,
    text       TEXT NOT NULL,
    PRIMARY KEY (meeting_id, kind, seq)
);
CREATE TABLE IF NOT EXISTS speakers (
    meeting_id TEXT NOT NULL REFERENCES meetings (id) ON DELETE CASCADE,
    seq        INTEGER NOT NULL,
    speaker    TEXT NOT NULL,
    name       TEXT,
    PRIMARY KEY (meeting_id, seq)
);
CREATE TABLE IF NOT EXISTS notes (
    meeting_id TEXT PRIMARY KEY REFERENCES meetings (id) ON DELETE CASCADE,
    my_notes   TEXT NOT NULL DEFAULT '',
    summary    TEXT,
    summary_md TEXT
);
CREATE TABLE IF NOT EXISTS people (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS keyterms (term TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS providers (
    id     TEXT PRIMARY KEY,
    kind   TEXT NOT NULL,
    config TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Titles and speaker names (kind 'meta'), segments ('live', 'refined') and
-- the summary ('summary'), one row each.
CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5 (
    meeting_id UNINDEXED,
    kind UNINDEXED,
    text,
    tokenize = 'unicode61 remove_diacritics 2'
);
";

/// Open `app.db` at `path` (WAL, foreign keys) with every table.
pub(crate) fn open(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(SCHEMA)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(conn)
}

fn ms(v: i64) -> u64 {
    v.max(0) as u64
}

pub(crate) fn exists(conn: &Connection, id: &MeetingId) -> Result<bool> {
    Ok(conn.query_row("SELECT 1 FROM meetings WHERE id = ?1", [id.as_str()], |_| Ok(())).optional()?.is_some())
}

/// Write `info` (insert or replace) with its speakers and its searchable
/// title and names.
pub(crate) fn put_info(tx: &Transaction, info: &MeetingInfo) -> Result<()> {
    let id = info.id.as_str();
    tx.execute(
        "INSERT INTO meetings (id, title, started_ms, duration_ms, status) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (id) DO UPDATE SET title = ?2, started_ms = ?3, duration_ms = ?4, status = ?5",
        params![id, info.title, info.started_ms as i64, info.duration_ms as i64, info.status.as_str()],
    )?;
    tx.execute("DELETE FROM speakers WHERE meeting_id = ?1", [id])?;
    for (seq, speaker) in info.speakers.iter().enumerate() {
        tx.execute(
            "INSERT INTO speakers (meeting_id, seq, speaker, name) VALUES (?1, ?2, ?3, ?4)",
            params![id, seq as i64, speaker.id, speaker.name],
        )?;
    }
    let names: Vec<&str> = info.speakers.iter().filter_map(|s| s.name.as_deref()).collect();
    set_search(tx, &info.id, "meta", &[format!("{} {}", info.title, names.join(" "))])
}

fn set_search(tx: &Transaction, id: &MeetingId, kind: &str, texts: &[String]) -> Result<()> {
    tx.execute("DELETE FROM search WHERE meeting_id = ?1 AND kind = ?2", params![id.as_str(), kind])?;
    add_search(tx, id, kind, texts)
}

fn add_search(tx: &Transaction, id: &MeetingId, kind: &str, texts: &[String]) -> Result<()> {
    let mut insert = tx.prepare_cached("INSERT INTO search (meeting_id, kind, text) VALUES (?1, ?2, ?3)")?;
    for text in texts.iter().filter(|t| !t.trim().is_empty()) {
        insert.execute(params![id.as_str(), kind, text])?;
    }
    Ok(())
}

/// Add `segments` after the transcript's last.
pub(crate) fn add_segments(tx: &Transaction, id: &MeetingId, kind: TranscriptKind, segments: &[Segment]) -> Result<()> {
    let next: i64 = tx.query_row(
        "SELECT coalesce(max(seq) + 1, 0) FROM segments WHERE meeting_id = ?1 AND kind = ?2",
        params![id.as_str(), kind.as_str()],
        |r| r.get(0),
    )?;
    let mut insert = tx.prepare_cached(
        "INSERT INTO segments (meeting_id, kind, seq, source, speaker, start_ms, end_ms, text)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for (i, s) in segments.iter().enumerate() {
        insert.execute(params![
            id.as_str(),
            kind.as_str(),
            next + i as i64,
            s.source.as_str(),
            s.speaker,
            s.start_ms as i64,
            s.end_ms as i64,
            s.text
        ])?;
    }
    let texts: Vec<String> = segments.iter().map(|s| s.text.clone()).collect();
    add_search(tx, id, kind.as_str(), &texts)
}

/// Replace a transcript with `segments`.
pub(crate) fn set_segments(tx: &Transaction, id: &MeetingId, kind: TranscriptKind, segments: &[Segment]) -> Result<()> {
    tx.execute("DELETE FROM segments WHERE meeting_id = ?1 AND kind = ?2", params![id.as_str(), kind.as_str()])?;
    tx.execute("DELETE FROM search WHERE meeting_id = ?1 AND kind = ?2", params![id.as_str(), kind.as_str()])?;
    add_segments(tx, id, kind, segments)
}

pub(crate) fn set_my_notes(tx: &Transaction, id: &MeetingId, text: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO notes (meeting_id, my_notes) VALUES (?1, ?2)
         ON CONFLICT (meeting_id) DO UPDATE SET my_notes = ?2",
        params![id.as_str(), text],
    )?;
    Ok(())
}

pub(crate) fn set_summary(tx: &Transaction, id: &MeetingId, summary: &Summary) -> Result<()> {
    let json = serde_json::to_string(summary).expect("a summary serializes");
    tx.execute(
        "INSERT INTO notes (meeting_id, summary, summary_md) VALUES (?1, ?2, ?3)
         ON CONFLICT (meeting_id) DO UPDATE SET summary = ?2, summary_md = ?3",
        params![id.as_str(), json, summary.markdown],
    )?;
    set_search(tx, id, "summary", &[summary.markdown.clone()])
}

pub(crate) fn delete(tx: &Transaction, id: &MeetingId) -> Result<()> {
    tx.execute("DELETE FROM meetings WHERE id = ?1", [id.as_str()])?;
    tx.execute("DELETE FROM search WHERE meeting_id = ?1", [id.as_str()])?;
    Ok(())
}

/// Index a whole folder (rebuild).
pub(crate) fn put_folder(tx: &Transaction, folder: &Folder) -> Result<()> {
    let id = &folder.info.id;
    put_info(tx, &folder.info)?;
    set_segments(tx, id, TranscriptKind::Live, &folder.live)?;
    set_segments(tx, id, TranscriptKind::Refined, &folder.refined)?;
    if !folder.my_notes.is_empty() {
        set_my_notes(tx, id, &folder.my_notes)?;
    }
    if let Some(summary) = &folder.summary {
        set_summary(tx, id, summary)?;
    }
    Ok(())
}

/// Forget every meeting (before a rebuild). The app's own tables stay.
pub(crate) fn clear_meetings(tx: &Transaction) -> Result<()> {
    tx.execute("DELETE FROM meetings", [])?;
    tx.execute("DELETE FROM search", [])?;
    Ok(())
}

fn speakers(conn: &Connection, id: &str) -> Result<Vec<Speaker>> {
    let mut stmt = conn.prepare_cached("SELECT speaker, name FROM speakers WHERE meeting_id = ?1 ORDER BY seq")?;
    let rows = stmt.query_map([id], |r| Ok(Speaker { id: r.get(0)?, name: r.get(1)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn infos(conn: &Connection, filter: &str, arg: Option<&str>) -> Result<Vec<MeetingInfo>> {
    let sql = format!(
        "SELECT id, title, started_ms, duration_ms, status FROM meetings {filter} ORDER BY started_ms DESC, id DESC"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let map = |r: &rusqlite::Row| {
        Ok(MeetingInfo {
            id: MeetingId(r.get(0)?),
            title: r.get(1)?,
            started_ms: ms(r.get(2)?),
            duration_ms: ms(r.get(3)?),
            status: Status::parse(&r.get::<_, String>(4)?),
            speakers: Vec::new(),
        })
    };
    let mut out: Vec<MeetingInfo> = match arg {
        Some(arg) => stmt.query_map([arg], map)?.collect::<Result<_, _>>()?,
        None => stmt.query_map([], map)?.collect::<Result<_, _>>()?,
    };
    for info in &mut out {
        info.speakers = speakers(conn, info.id.as_str())?;
    }
    Ok(out)
}

pub(crate) fn list(conn: &Connection) -> Result<Vec<MeetingInfo>> {
    infos(conn, "", None)
}

pub(crate) fn info(conn: &Connection, id: &MeetingId) -> Result<Option<MeetingInfo>> {
    Ok(infos(conn, "WHERE id = ?1", Some(id.as_str()))?.pop())
}

pub(crate) fn meeting(conn: &Connection, id: &MeetingId) -> Result<Option<Meeting>> {
    let Some(info) = info(conn, id)? else {
        return Ok(None);
    };
    let notes: Option<(String, Option<String>)> = conn
        .query_row("SELECT my_notes, summary FROM notes WHERE meeting_id = ?1", [id.as_str()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let (my_notes, summary) = notes.unwrap_or_default();
    let summary = summary.and_then(|json| serde_json::from_str(&json).ok());
    Ok(Some(Meeting { info, summary, my_notes }))
}

pub(crate) fn segments(conn: &Connection, id: &MeetingId, kind: TranscriptKind) -> Result<Vec<Segment>> {
    let mut stmt = conn.prepare_cached(
        "SELECT source, speaker, start_ms, end_ms, text FROM segments
         WHERE meeting_id = ?1 AND kind = ?2 ORDER BY seq",
    )?;
    let rows = stmt.query_map(params![id.as_str(), kind.as_str()], |r| {
        let source: String = r.get(0)?;
        Ok(Segment {
            source: if source == "me" { recap_engine::Source::Me } else { recap_engine::Source::Them },
            speaker: r.get(1)?,
            start_ms: ms(r.get(2)?),
            end_ms: ms(r.get(3)?),
            text: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// `query` as an FTS5 query: each word a quoted prefix, all of them needed.
fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// The meetings matching `query`, best match first.
pub(crate) fn search(conn: &Connection, query: &str) -> Result<Vec<MeetingId>> {
    let Some(fts) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare_cached("SELECT meeting_id FROM search WHERE search MATCH ?1 ORDER BY rank")?;
    let mut out: Vec<MeetingId> = Vec::new();
    for id in stmt.query_map([fts], |r| r.get::<_, String>(0))? {
        let id = MeetingId(id?);
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}
