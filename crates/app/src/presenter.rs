//! Engine events in, the UI's shared state out (`ui/host.luau` holds the
//! shapes). Plain data, no zgx: the host converts it and sets the keys.
//!
//! Only what changes travels: the meeting, the row count and the rows still
//! partial. A final row never changes again, so the UI asks for it once
//! (`transcript_row`, [`Presenter::row`]).
//!
//! A stored meeting ([`Presenter::stored`]) reads the same way, in read
//! mode: stopped, every row final, rows of named speakers carrying the name.

use recap_engine::{EngineEvent, LiveTranscript, MeetingState, Row, Segment, SegmentId, Source};
use recap_storage::{MeetingInfo, Speaker};
use serde_json::{Map, Value, json};

use crate::home::speaker_label;

/// How many lines the overlay shows.
pub const CAPTION_LINES: usize = 3;

/// Which shared keys an event changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    pub meeting: bool,
    pub transcript: bool,
    pub captions: bool,
}

enum Rows {
    /// A running meeting's live transcript.
    Live(LiveTranscript),
    /// A stored meeting's rows, final.
    Stored(Vec<Value>),
}

pub struct Presenter {
    rows: Rows,
    state: Option<MeetingState>,
    elapsed_ms: u64,
    title: String,
    started_ms: u64,
    /// The stored meeting's id; empty while live.
    id: String,
}

impl Presenter {
    /// A meeting that started at `started_ms` (ms since the epoch).
    pub fn new(started_ms: u64) -> Self {
        Self {
            rows: Rows::Live(LiveTranscript::default()),
            state: None,
            elapsed_ms: 0,
            title: String::new(),
            started_ms,
            id: String::new(),
        }
    }

    /// A stored meeting in read mode, with its transcript (refined or live).
    /// Refined segments keep their speakers apart; live ones group by side as
    /// they did live.
    pub fn stored(info: &MeetingInfo, segments: &[recap_storage::Segment]) -> Self {
        let rows = if segments.iter().any(|s| s.speaker.is_some()) {
            speaker_rows(info, segments)
        } else {
            let mut live = LiveTranscript::default();
            for (i, s) in segments.iter().enumerate() {
                live.apply(&Segment {
                    id: SegmentId(i as u64),
                    source: s.source,
                    start_ms: s.start_ms,
                    end_ms: s.end_ms,
                    text: s.text.clone(),
                    is_final: true,
                });
            }
            live.rows().iter().map(row_value).collect()
        };
        Self {
            rows: Rows::Stored(rows),
            state: Some(MeetingState::Stopped),
            elapsed_ms: info.duration_ms,
            title: info.title.clone(),
            started_ms: info.started_ms,
            id: info.id.to_string(),
        }
    }

    pub fn apply(&mut self, event: &EngineEvent) -> Changed {
        let Rows::Live(transcript) = &mut self.rows else {
            return Changed::default();
        };
        match event {
            EngineEvent::State(state) => {
                self.state = Some(*state);
                let finished = *state == MeetingState::Stopped && !transcript.finish().is_empty();
                Changed { meeting: true, transcript: finished, captions: finished }
            }
            EngineEvent::Elapsed(ms) => {
                self.elapsed_ms = *ms;
                Changed { meeting: true, ..Default::default() }
            }
            EngineEvent::Segment(segment) => {
                transcript.apply(segment);
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
            "id": self.id,
        })
    }

    /// `transcript`: the count, and the open rows by index (from 1, as text).
    pub fn transcript(&self) -> Value {
        match &self.rows {
            Rows::Live(transcript) => {
                let open: Map<String, Value> = transcript
                    .open_rows()
                    .filter_map(|i| transcript.row(i))
                    .map(|row| ((row.index + 1).to_string(), row_value(row)))
                    .collect();
                json!({ "count": transcript.len(), "open": open })
            }
            Rows::Stored(rows) => json!({ "count": rows.len(), "open": {} }),
        }
    }

    /// Row `index` (from 1), or null.
    pub fn row(&self, index: usize) -> Value {
        let Some(i) = index.checked_sub(1) else {
            return Value::Null;
        };
        match &self.rows {
            Rows::Live(transcript) => transcript.row(i).map_or(Value::Null, row_value),
            Rows::Stored(rows) => rows.get(i).cloned().unwrap_or(Value::Null),
        }
    }

    /// `captions`: the last lines of Them, oldest first.
    pub fn captions(&self) -> Value {
        let Rows::Live(transcript) = &self.rows else {
            return json!([]);
        };
        let mut lines: Vec<Value> = transcript
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

/// One row per turn of a speaker: a speaker's segments in a row join.
fn speaker_rows(info: &MeetingInfo, segments: &[recap_storage::Segment]) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    let mut last: Option<(&str, Source)> = None;
    for s in segments {
        let speaker = s.speaker.as_deref().unwrap_or("");
        if last == Some((speaker, s.source))
            && let Some(Value::String(text)) = rows.last_mut().and_then(|row| row.get_mut("final"))
        {
            text.push(' ');
            text.push_str(&s.text);
            continue;
        }
        last = Some((speaker, s.source));
        let place = info.speakers.iter().position(|sp| sp.id == speaker);
        let label = match place {
            Some(i) => speaker_label(&info.speakers[i]),
            None => speaker_label(&Speaker { id: speaker.to_owned(), name: None }),
        };
        rows.push(json!({
            "index": rows.len() + 1,
            "source": s.source.as_str(),
            "start_ms": s.start_ms,
            "final": s.text,
            "partial": "",
            "open": false,
            "speaker": label,
            "speaker_n": place.map_or(0, |i| i + 1),
        }));
    }
    rows
}
