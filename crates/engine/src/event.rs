use serde::{Deserialize, Serialize};

use crate::{LiveStatus, ProviderError, Row};

/// One of the two audio sources (GLOSSARY: Me / Them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The mic: the user.
    Me,
    /// The meeting audio (system loopback): the people the user meets with.
    Them,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Me => "me",
            Source::Them => "them",
        }
    }
}

/// A segment's identity: a partial and the final it becomes share it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SegmentId(pub u64);

/// One unit of transcript text (GLOSSARY: Segment). Times are milliseconds of
/// meeting time: audio heard while recording, pauses left out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub id: SegmentId,
    pub source: Source,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    /// Final (committed) text; otherwise a partial the engine may still revise.
    pub is_final: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingState {
    Recording,
    Paused,
    /// Stop pressed or the audio ended; `Finished` follows.
    Stopped,
}

impl MeetingState {
    pub fn as_str(self) -> &'static str {
        match self {
            MeetingState::Recording => "recording",
            MeetingState::Paused => "paused",
            MeetingState::Stopped => "stopped",
        }
    }
}

/// What a running meeting tells whoever listens (the app's UI, a test).
#[derive(Clone, Debug)]
pub enum EngineEvent {
    State(MeetingState),
    /// A partial, or the final it became (same id).
    Segment(Segment),
    /// A final kept for the transcript (save it). Them's finals are kept at
    /// once; Me's once Them has caught up and it is no echo.
    Committed(Segment),
    /// A Me segment shown turned out an echo of Them on the mic: take it
    /// back. It is never committed.
    Dropped(SegmentId),
    /// The meeting resumed (or stopped) after a pause: the marker row.
    Pause(PauseMark),
    /// Meeting time, once a second of audio while recording.
    Elapsed(u64),
    /// How one side's live captions are doing: which provider captions it
    /// (told as each side starts, and when a backup takes over), and
    /// whether it is reconnecting.
    Live {
        source: Source,
        status: LiveStatus,
    },
    /// A provider failed for one side, told as it happens; recording goes
    /// on, that side uncaptioned until `MeetingControl::replace_live`.
    Error {
        source: Source,
        error: ProviderError,
    },
    /// The recording could not be written (disk full, folder gone): what is
    /// on disk so far stays; captions go on. Also when the audio can't be
    /// read at all (no thread for it): the meeting ends.
    RecordingFailed(String),
    /// The last event: what the meeting produced.
    Finished(MeetingResult),
}

/// A pause in a meeting: one meeting goes on after it, with this marker
/// where it was ("15:54 – 15:58").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseMark {
    /// Meeting time it paused at (pauses are left out of meeting time).
    pub at_ms: u64,
    /// Wall clock, ms since the epoch: when it paused and resumed.
    pub paused_ms: u64,
    pub resumed_ms: u64,
}

/// What a meeting produces. After-Stop processing (refined transcript,
/// speakers, summary) extends this in later tickets.
#[derive(Clone, Debug, Default)]
pub struct MeetingResult {
    /// Its pauses, in order.
    pub pauses: Vec<PauseMark>,
    /// The final live segments, in the order they were committed.
    pub segments: Vec<Segment>,
    /// The live transcript's rows (turns).
    pub rows: Vec<Row>,
    /// Meeting time recorded, pauses left out.
    pub duration_ms: u64,
}
