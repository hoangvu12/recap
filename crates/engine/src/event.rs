use serde::{Deserialize, Serialize};

use crate::{ProviderError, Row};

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
    /// Meeting time, once a second of audio while recording.
    Elapsed(u64),
    /// A provider failed for one side; recording goes on.
    Error { source: Source, error: ProviderError },
    /// The last event: what the meeting produced.
    Finished(MeetingResult),
}

/// What a meeting produces. After-Stop processing (refined transcript,
/// speakers, summary) extends this in later tickets.
#[derive(Clone, Debug, Default)]
pub struct MeetingResult {
    /// The final live segments, in the order they were committed.
    pub segments: Vec<Segment>,
    /// The live transcript's rows (turns).
    pub rows: Vec<Row>,
    /// Meeting time recorded, pauses left out.
    pub duration_ms: u64,
}
