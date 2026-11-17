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

/// The known people's file's name (the app keeps it in Recap's home,
/// beside settings.json; the store's root unless given).
pub const PEOPLE: &str = "people.json";

/// Recap's meetings on disk: the folders and the database that indexes them.
/// Clones share one connection; every call is one short transaction.
#[derive(Clone)]
pub struct Store {
    root: Arc<PathBuf>,
    conn: Arc<Mutex<Connection>>,
    /// Where the known people are kept beside the database (it may be lost).
    people_file: Arc<PathBuf>,
}

impl Store {
    /// `%LOCALAPPDATA%\Recap`.
    pub fn default_root() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Recap"))
    }

    /// The store at `root`, made if new. Without `app.db` the database is
    /// rebuilt from the meeting folders. Known people are kept in
    /// `root/people.json`.
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_with_people(root, &root.join(PEOPLE))
    }

    /// The store at `root`, its known people kept in `people_file` (the app:
    /// Recap's home, beside settings.json, wherever the meetings are).
    pub fn open_with_people(root: &Path, people_file: &Path) -> Result<Self> {
        files::ensure_dir(&root.join("meetings"))?;
        let db_path = root.join("app.db");
        let fresh = !db_path.exists();
        let store = Self {
            root: Arc::new(root.to_owned()),
            conn: Arc::new(Mutex::new(db::open(&db_path)?)),
            people_file: Arc::new(people_file.to_owned()),
        };
        if fresh {
            store.rebuild()?;
        } else if !people_file.exists() {
            // A store from before people had a file: give them one.
            store.keep_people(&store.conn())?;
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

    /// Meeting `id`'s recording (`audio.ogg`), whether or not it exists.
    pub fn recording(&self, id: &MeetingId) -> PathBuf {
        self.folder(id).join(files::AUDIO)
    }

    /// Whether meeting `id` has its recording still (Recover needs it).
    pub fn has_recording(&self, id: &MeetingId) -> bool {
        self.recording(id).is_file()
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
        if info.speakers.iter().any(|s| s.name.is_some()) {
            self.keep_people(&conn)?;
        }
        Ok(info)
    }

    /// The known people, written to their file (a name may be new).
    fn keep_people(&self, conn: &Connection) -> Result<()> {
        let people = db::people(conn)?;
        if files::read_json::<Vec<String>>(&self.people_file)?.as_ref() != Some(&people) {
            files::write_json(&self.people_file, &people)?;
        }
        Ok(())
    }

    /// Name speaker `speaker` (`me`, `S1`..) of meeting `id`, or unname it
    /// (`None` or blank). The name, trimmed, becomes a known person. The
    /// transcript keeps its speaker ids: every view labels them from here.
    pub fn rename_speaker(&self, id: &MeetingId, speaker: &str, name: Option<&str>) -> Result<MeetingInfo> {
        let name = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned);
        let mut found = false;
        let info = self.update(id, |info| {
            if let Some(s) = info.speakers.iter_mut().find(|s| s.id == speaker) {
                s.name = name.clone();
                found = true;
            }
        })?;
        if found { Ok(info) } else { Err(StoreError::NoSpeaker { meeting: id.clone(), speaker: speaker.to_owned() }) }
    }

    /// Everyone a meeting named (rename suggestions), by name.
    pub fn people(&self) -> Result<Vec<String>> {
        db::people(&self.conn())
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
    /// app's own tables (people, keyterms, providers, settings) stay, and the
    /// people in their file come back (a lost database loses none).
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
        let kept = files::read_json::<Vec<String>>(&self.people_file)?.unwrap_or_default();
        let tx = conn.transaction()?;
        db::clear_meetings(&tx)?;
        for name in &kept {
            db::add_person(&tx, name)?;
        }
        for folder in &found {
            db::put_folder(&tx, folder)?;
        }
        tx.commit()?;
        self.keep_people(&conn)
    }

    /// Audio retention: delete `audio.ogg` of finished meetings that started
    /// more than `keep_days` days before `now_ms` (0 keeps all). Transcripts,
    /// summaries and the speech engines' files stay; unfinished meetings
    /// keep their audio for Recover. The meetings whose audio went.
    pub fn prune_audio(&self, now_ms: u64, keep_days: u32) -> Result<Vec<MeetingId>> {
        if keep_days == 0 {
            return Ok(Vec::new());
        }
        let cutoff = now_ms.saturating_sub(u64::from(keep_days) * 86_400_000);
        let mut pruned = Vec::new();
        for info in self.list()? {
            if info.status != Status::Done || info.started_ms >= cutoff {
                continue;
            }
            if files::remove_file(&self.folder(&info.id).join(files::AUDIO))? {
                pruned.push(info.id);
            }
        }
        pruned.sort();
        Ok(pruned)
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
