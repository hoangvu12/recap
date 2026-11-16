//! Engine events in, the UI's shared state out (`ui/host.luau` holds the
//! shapes). Plain data, no zgx: the host converts it and sets the keys.
//!
//! Only what changes travels: the meeting, the row count and the rows still
//! partial. A final row never changes again, so the UI asks for it once
//! (`transcript_row`, [`Presenter::row`]).

use recap_engine::{EngineEvent, LiveTranscript, MeetingState, Row, Source};
use serde_json::{Map, Value, json};

/// How many lines the overlay shows.
pub const CAPTION_LINES: usize = 3;

/// Which shared keys an event changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    pub meeting: bool,
    pub transcript: bool,
    pub captions: bool,
}

pub struct Presenter {
    transcript: LiveTranscript,
    state: Option<MeetingState>,
    elapsed_ms: u64,
    title: String,
    started_ms: u64,
}

impl Presenter {
    /// A meeting that started at `started_ms` (ms since the epoch).
    pub fn new(started_ms: u64) -> Self {
        Self {
            transcript: LiveTranscript::default(),
            state: None,
            elapsed_ms: 0,
            title: String::new(),
            started_ms,
        }
    }

    pub fn apply(&mut self, event: &EngineEvent) -> Changed {
        match event {
            EngineEvent::State(state) => {
                self.state = Some(*state);
                let finished = *state == MeetingState::Stopped && !self.transcript.finish().is_empty();
                Changed { meeting: true, transcript: finished, captions: finished }
            }
            EngineEvent::Elapsed(ms) => {
                self.elapsed_ms = *ms;
                Changed { meeting: true, ..Default::default() }
            }
            EngineEvent::Segment(segment) => {
                self.transcript.apply(segment);
                Changed { meeting: false, transcript: true, captions: segment.source == Source::Them }
            }
            EngineEvent::Error { .. } | EngineEvent::RecordingFailed(_) => Changed::default(),
            EngineEvent::Finished(result) => {
                self.elapsed_ms = self.elapsed_ms.max(result.duration_ms);
                Changed { meeting: true, ..Default::default() }
            }
        }
    }

    /// `meeting`.
    pub fn meeting(&self) -> Value {
        json!({
            "state": self.state.map_or("idle", MeetingState::as_str),
            "elapsed_ms": self.elapsed_ms,
            "title": self.title,
            "started_ms": self.started_ms,
        })
    }

    /// `transcript`: the count, and the open rows by index (from 1, as text).
    pub fn transcript(&self) -> Value {
        let open: Map<String, Value> = self
            .transcript
            .open_rows()
            .filter_map(|i| self.transcript.row(i))
            .map(|row| ((row.index + 1).to_string(), row_value(row)))
            .collect();
        json!({ "count": self.transcript.len(), "open": open })
    }

    /// Row `index` (from 1), or null.
    pub fn row(&self, index: usize) -> Value {
        index
            .checked_sub(1)
            .and_then(|i| self.transcript.row(i))
            .map_or(Value::Null, row_value)
    }

    /// `captions`: the last lines of Them, oldest first.
    pub fn captions(&self) -> Value {
        let mut lines: Vec<Value> = self
            .transcript
            .rows()
            .iter()
            .rev()
            .filter(|row| row.source == Source::Them)
            .take(CAPTION_LINES)
            .map(|row| json!({ "source": "them", "final": row.final_text, "partial": row.partial_text }))
            .collect();
        lines.reverse();
        Value::Array(lines)
    }
}

fn row_value(row: &Row) -> Value {
    json!({
        "index": row.index + 1,
        "source": row.source.as_str(),
        "start_ms": row.start_ms,
        "final": row.final_text,
        "partial": row.partial_text,
        "open": row.is_open(),
    })
}
