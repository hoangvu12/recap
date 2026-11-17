//! Recap's storage: every meeting has a folder, and `app.db` indexes them.
//!
//! ```text
//! <root>                       %LOCALAPPDATA%\Recap  (Store::default_root)
//!   app.db                     SQLite, WAL, FTS5: an index, rebuilt from the folders
//!   meetings/<id>/
//!     info.json                title, start, duration, status, speakers
//!     live.ndjson              live segments, appended as they turn final
//!     refined.ndjson           the refined transcript, replaced as a whole
//!     my_notes.md              what the user typed
//!     summary.json, summary.md the summary (data, overview, Markdown)
//!     audio.ogg, stt_*.json    other writers' files (the recorder, the pipeline)
//! ```
//!
//! **The folders are the truth.** Each write goes to the folder first, then
//! to the database. If the database is lost or behind, [`Store::open`] (when
//! `app.db` is missing) or [`Store::rebuild`] reads it all back from the
//! folders. A live file cut off mid-line by a crash rebuilds up to the cut.
//!
//! [`Store`] is the whole interface. Writers: [`Store::create`] a meeting,
//! [`Store::append_live`] segments while it runs, [`Store::set_refined`],
//! [`Store::update`] its info (title, duration, status, speakers) and
//! [`Store::set_summary`] after Stop. Readers: [`Store::list`],
//! [`Store::meeting`], [`Store::transcript`] / [`Store::segments`] and
//! [`Store::search`]. It is cheap to clone and shared across threads.
//! [`Store::prune_audio`] applies audio retention. [`Store::rename_speaker`]
//! names a voice; every name a meeting gives is a known person
//! ([`Store::people`], the rename suggestions), kept in `people.json` too
//! ([`PEOPLE`]; the app keeps it beside settings.json) so a lost database
//! loses none of them.
//!
//! The user's [`Settings`] live in `settings.json` in Recap's home folder
//! (beside the store unless a storage folder moves the meetings away).

mod db;
mod files;
mod id;
pub mod settings;
mod store;

use recap_engine::Source;
use serde::{Deserialize, Serialize};

pub use settings::{GlossaryError, Hotkeys, MeetingAudio, Settings, Theme};
pub use store::{PEOPLE, Store};

/// A meeting's id: its folder's name (when it started, UTC, `20261116-100000`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MeetingId(String);

impl MeetingId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MeetingId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl From<String> for MeetingId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for MeetingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a meeting is in its life. A meeting that is not `Done` and not the
/// one recording now is unfinished (Home offers Recover).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Recording,
    /// Stopped; the after-Stop processing has not finished.
    Processing,
    Done,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Recording => "recording",
            Status::Processing => "processing",
            Status::Done => "done",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "done" => Status::Done,
            "processing" => Status::Processing,
            _ => Status::Recording,
        }
    }
}

/// A speaker of a meeting: `me`, or a diarized voice `S1..Sn` (GLOSSARY:
/// Speaker), named or not ("Speaker N").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speaker {
    pub id: String,
    pub name: Option<String>,
}

/// What a meeting is, without its transcript: what lists show.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingInfo {
    pub id: MeetingId,
    /// Empty until one is given (the summary step names it).
    pub title: String,
    /// Ms since the epoch.
    pub started_ms: u64,
    /// Meeting time recorded, pauses left out.
    pub duration_ms: u64,
    pub status: Status,
    /// In the order they are shown.
    pub speakers: Vec<Speaker>,
}

/// One stored transcript segment (GLOSSARY: Segment). Live segments have no
/// speaker; refined ones have `me` or `S1..Sn`. Times are meeting time, ms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Which transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptKind {
    /// Written while the meeting runs.
    Live,
    /// Written after Stop; replaces the live one when there is one.
    Refined,
}

impl TranscriptKind {
    fn as_str(self) -> &'static str {
        match self {
            TranscriptKind::Live => "live",
            TranscriptKind::Refined => "refined",
        }
    }
}

/// A meeting's summary (GLOSSARY: Summary): its overview (Home's detail
/// pane), the Markdown shown and exported, and the structured data the
/// summary step returned (topics, next steps, decisions), kept as is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub overview: String,
    pub markdown: String,
    pub data: serde_json::Value,
}

/// A meeting with what was written after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Meeting {
    pub info: MeetingInfo,
    pub summary: Option<Summary>,
    /// GLOSSARY: My notes.
    pub my_notes: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no meeting {0}")]
    NoMeeting(MeetingId),
    #[error("meeting {meeting} has no speaker {speaker}")]
    NoSpeaker { meeting: MeetingId, speaker: String },
    #[error("{path}: {source}")]
    Io { path: std::path::PathBuf, source: std::io::Error },
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{path}: {source}")]
    Json { path: std::path::PathBuf, source: serde_json::Error },
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;
