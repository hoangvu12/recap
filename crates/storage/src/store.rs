use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use rusqlite::Connection;

use crate::{
    Meeting, MeetingId, MeetingInfo, Result, Segment, Status, StoreError, Summary, TranscriptKind, db,
    files::{self, INFO, MY_NOTES, SUMMARY, SUMMARY_MD},
    id,
};

/// Recap's meetings on disk: the folders and the database that indexes them.
/// Clones share one connection; every call is one short transaction.
#[derive(Clone)]
pub struct Store {
    root: Arc<PathBuf>,
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    /// `%LOCALAPPDATA%\Recap`.
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Recap"))
    }

    /// The store at `root`, made if new. Without `app.db` the database is
    /// rebuilt from the meeting folders.
    pub fn open(root: &Path) -> Result<Self> {
        files::ensure_dir(&root.join("meetings"))?;
        let db_path = root.join("app.db");
        let fresh = !db_path.exists();
        let store = Self { root: Arc::new(root.to_owned()), conn: Arc::new(Mutex::new(db::open(&db_path)?)) };
        if fresh {
            store.rebuild()?;
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Meeting `id`'s folder (other writers put audio.ogg and stt_*.json here).
    pub fn folder(&self, id: &MeetingId) -> PathBuf {
        self.root.join("meetings").join(id.as_str())
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic mid-write leaves the connection usable: the transaction
        // rolled back when it dropped.
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The connection, after checking `id` is a meeting.
    fn conn_for(&self, id: &MeetingId) -> Result<MutexGuard<'_, Connection>> {
        let conn = self.conn();
        if db::exists(&conn, id)? { Ok(conn) } else { Err(StoreError::NoMeeting(id.clone())) }
    }

    /// A new meeting that started at `started_ms`, recording, untitled.
    pub fn create(&self, started_ms: u64) -> Result<MeetingInfo> {
        let mut conn = self.conn();
        let base = id::base(started_ms);
        let mut n = 1;
        let id = loop {
            let id = MeetingId(if n == 1 { base.clone() } else { format!("{base}-{n}") });
            match files::create_dir(&self.folder(&id)) {
                Ok(()) => break id,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                Err(source) => return Err(StoreError::Io { path: self.folder(&id), source }),
            }
        };
        let info = MeetingInfo {
            id,
            title: String::new(),
            started_ms,
            duration_ms: 0,
            status: Status::Recording,
            speakers: Vec::new(),
        };
        files::write_json(&self.folder(&info.id).join(INFO), &info)?;
        let tx = conn.transaction()?;
        db::put_info(&tx, &info)?;
        tx.commit()?;
        Ok(info)
    }

    /// Change meeting `id`'s info (its id stays); the info it now has.
    pub fn update(&self, id: &MeetingId, change: impl FnOnce(&mut MeetingInfo)) -> Result<MeetingInfo> {
        let mut conn = self.conn_for(id)?;
        let path = self.folder(id).join(INFO);
        let mut info = match files::read_json::<MeetingInfo>(&path)? {
            Some(info) => info,
            None => db::info(&conn, id)?.ok_or_else(|| StoreError::NoMeeting(id.clone()))?,
        };
        change(&mut info);
        info.id = id.clone();
        files::write_json(&path, &info)?;
        let tx = conn.transaction()?;
        db::put_info(&tx, &info)?;
        tx.commit()?;
        Ok(info)
    }

    /// Add live segments (final ones, as they come) to meeting `id`.
    pub fn append_live(&self, id: &MeetingId, segments: &[Segment]) -> Result<()> {
        let mut conn = self.conn_for(id)?;
        files::append_segments(&self.folder(id).join(files::transcript_file(TranscriptKind::Live)), segments)?;
        let tx = conn.transaction()?;
        db::add_segments(&tx, id, TranscriptKind::Live, segments)?;
        tx.commit()?;
        Ok(())
    }

    /// Meeting `id`'s refined transcript, replacing any before it.
    pub fn set_refined(&self, id: &MeetingId, segments: &[Segment]) -> Result<()> {
        let mut conn = self.conn_for(id)?;
        files::write_segments(&self.folder(id).join(files::transcript_file(TranscriptKind::Refined)), segments)?;
        let tx = conn.transaction()?;
        db::set_segments(&tx, id, TranscriptKind::Refined, segments)?;
        tx.commit()?;
        Ok(())
    }

    /// What the user typed in the Notes tab.
    pub fn set_my_notes(&self, id: &MeetingId, text: &str) -> Result<()> {
        let mut conn = self.conn_for(id)?;
        files::write_text(&self.folder(id).join(MY_NOTES), text)?;
        let tx = conn.transaction()?;
        db::set_my_notes(&tx, id, text)?;
        tx.commit()?;
        Ok(())
    }

    /// Meeting `id`'s summary; summary.md holds its Markdown.
    pub fn set_summary(&self, id: &MeetingId, summary: &Summary) -> Result<()> {
        let mut conn = self.conn_for(id)?;
        let folder = self.folder(id);
        files::write_json(&folder.join(SUMMARY), summary)?;
        files::write_text(&folder.join(SUMMARY_MD), &summary.markdown)?;
        let tx = conn.transaction()?;
        db::set_summary(&tx, id, summary)?;
        tx.commit()?;
        Ok(())
    }

    /// Delete meeting `id` and its folder.
    pub fn delete(&self, id: &MeetingId) -> Result<()> {
        let mut conn = self.conn();
        // The folder first: if it can't go, the meeting stays whole.
        files::remove_dir(&self.folder(id))?;
        let tx = conn.transaction()?;
        db::delete(&tx, id)?;
        tx.commit()?;
        Ok(())
    }

    /// Index every meeting folder again, as if the database were new. The
    /// app's own tables (people, keyterms, providers, settings) stay.
    pub fn rebuild(&self) -> Result<()> {
        let mut conn = self.conn();
        let mut found = Vec::new();
        for dir in files::meeting_dirs(&self.root.join("meetings"))? {
            if let Some(mut folder) = files::read_folder(&dir)? {
                // The folder's name is the id, whatever info.json says.
                if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
                    folder.info.id = MeetingId(name.to_owned());
                }
                found.push(folder);
            }
        }
        let tx = conn.transaction()?;
        db::clear_meetings(&tx)?;
        for folder in &found {
            db::put_folder(&tx, folder)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every meeting, newest first.
    pub fn list(&self) -> Result<Vec<MeetingInfo>> {
        db::list(&self.conn())
    }

    /// Meeting `id` with its summary and my notes.
    pub fn meeting(&self, id: &MeetingId) -> Result<Option<Meeting>> {
        db::meeting(&self.conn(), id)
    }

    /// One of meeting `id`'s transcripts, in order.
    pub fn segments(&self, id: &MeetingId, kind: TranscriptKind) -> Result<Vec<Segment>> {
        db::segments(&self.conn(), id, kind)
    }

    /// The transcript to show: the refined one once there is one, else the live one.
    pub fn transcript(&self, id: &MeetingId) -> Result<Vec<Segment>> {
        let conn = self.conn();
        let refined = db::segments(&conn, id, TranscriptKind::Refined)?;
        if refined.is_empty() { db::segments(&conn, id, TranscriptKind::Live) } else { Ok(refined) }
    }

    /// The meetings whose title, speakers, transcript or summary hold every
    /// word of `query` (a word's start is enough), best match first.
    pub fn search(&self, query: &str) -> Result<Vec<MeetingId>> {
        db::search(&self.conn(), query)
    }
}
