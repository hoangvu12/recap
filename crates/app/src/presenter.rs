//! Engine events in, the UI's shared state out (`ui/host.luau` holds the
//! shapes). Plain data, no zgx: the host converts it and sets the keys.
//!
//! Only what changes travels: the meeting, the row count and the rows still
//! partial. A final row never changes again, so the UI asks for it once
//! (`transcript_row`, [`Presenter::row`]). The exceptions are an echo taken
//! back (its row loses its words) and a pause marker, a row of its own.
//!
//! A stored meeting ([`Presenter::stored`]) reads the same way, in read
//! mode: stopped, every row final, rows of named speakers carrying the name,
//! its pauses as marker rows.

use std::collections::BTreeSet;

use recap_engine::{EngineEvent, LiveTranscript, MeetingState, PauseMark, Row, Segment, SegmentId, Source};
use recap_storage::{MeetingInfo, Speaker};
use serde_json::{Map, Value, json};

use crate::home::{self, speaker_label};

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
    /// The stored meeting's id; empty until it is saved.
    id: String,
    /// My lines are captions too ("Caption my own speech").
    my_captions: bool,
    /// What the user should know: a provider failed, the backup took over.
    notice: String,
    /// Final rows changed after the UI read them (echoes taken back).
    revised: BTreeSet<usize>,
    /// A refined transcript's speakers, as chips (`home::speakers`).
    speakers: Value,
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
            my_captions: false,
            notice: String::new(),
            revised: BTreeSet::new(),
            speakers: json!([]),
        }
    }

    /// Show my lines in the captions too (dimmed by the overlay).
    pub fn with_my_captions(mut self, on: bool) -> Self {
        self.my_captions = on;
        self
    }

    /// A stored meeting in read mode, with its transcript (refined or live).
    /// Refined segments keep their speakers apart; live ones group by side as
    /// they did live.
    pub fn stored(info: &MeetingInfo, segments: &[recap_storage::Segment]) -> Self {
        Self::stored_with_pauses(info, segments, &[])
    }

    /// [`Presenter::stored`] with the meeting's pauses (`pauses.json`): a
    /// marker row where each was, before what was said after it.
    pub fn stored_with_pauses(info: &MeetingInfo, segments: &[recap_storage::Segment], pauses: &[PauseMark]) -> Self {
        let mut pauses: Vec<PauseMark> = pauses.to_vec();
        pauses.sort_by_key(|p| p.at_ms);
        let rows = if segments.iter().any(|s| s.speaker.is_some()) {
            speaker_rows(info, segments, &pauses)
        } else {
            let mut live = LiveTranscript::default();
            let mut pauses = pauses.iter().peekable();
            for (i, s) in segments.iter().enumerate() {
                while let Some(mark) = pauses.next_if(|p| p.at_ms <= s.start_ms) {
                    live.mark_pause(*mark);
                }
                live.apply(&Segment {
                    id: SegmentId(i as u64),
                    source: s.source,
                    start_ms: s.start_ms,
                    end_ms: s.end_ms,
                    text: s.text.clone(),
                    is_final: true,
                });
            }
            for mark in pauses {
                live.mark_pause(*mark);
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
            my_captions: false,
            notice: String::new(),
            revised: BTreeSet::new(),
            speakers: if segments.iter().any(|s| s.speaker.is_some()) { home::speakers(info) } else { json!([]) },
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
                let captions = segment.source == Source::Them || self.my_captions;
                Changed { meeting: false, transcript: true, captions }
            }
            EngineEvent::Dropped(id) => {
                let open: Vec<usize> = transcript.open_rows().collect();
                let Some(index) = transcript.forget(*id) else {
                    return Changed::default();
                };
                // A row the UI holds as final: send it again. (One that was
                // open is read again once it leaves `open`.)
                if !open.contains(&index) {
                    self.revised.insert(index);
                }
                Changed { meeting: false, transcript: true, captions: true }
            }
            EngineEvent::Pause(mark) => {
                transcript.mark_pause(*mark);
                Changed { transcript: true, ..Default::default() }
            }
            EngineEvent::Error { error, .. } => {
                self.notice = error.message.clone();
                Changed { meeting: true, ..Default::default() }
            }
            EngineEvent::Committed(_) | EngineEvent::RecordingFailed(_) => Changed::default(),
            EngineEvent::Finished(result) => {
                self.elapsed_ms = self.elapsed_ms.max(result.duration_ms);
                Changed { meeting: true, ..Default::default() }
            }
        }
    }

    /// Tell the user `text` (the backup took over); `meeting` changed.
    pub fn notice(&mut self, text: &str) {
        self.notice = text.to_owned();
    }

    /// The meeting is stored as `id`; `meeting` changed.
    pub fn saved_as(&mut self, id: &str) {
        self.id = id.to_owned();
    }

    /// The meeting's state; none before it starts.
    pub fn state(&self) -> Option<MeetingState> {
        self.state
    }

    /// The stored meeting's id ("" until it is saved).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// `meeting`.
    pub fn meeting(&self) -> Value {
        json!({
            "state": self.state.map_or("idle", MeetingState::as_str),
            "elapsed_ms": self.elapsed_ms,
            "title": self.title,
            "started_ms": self.started_ms,
            "id": self.id,
            "notice": self.notice,
            "speakers": self.speakers,
        })
    }

    /// `transcript`: the count, and the open rows by index (from 1, as text).
    pub fn transcript(&self) -> Value {
        match &self.rows {
            Rows::Live(transcript) => {
                let rows = |indexes: &mut dyn Iterator<Item = usize>| -> Map<String, Value> {
                    indexes
                        .filter_map(|i| transcript.row(i))
                        .map(|row| ((row.index + 1).to_string(), row_value(row)))
                        .collect()
                };
                let mut value = json!({ "count": transcript.len(), "open": rows(&mut transcript.open_rows()) });
                let revised = rows(&mut self.revised.iter().copied().filter(|&i| !transcript.row(i).is_some_and(Row::is_open)));
                if !revised.is_empty() {
                    value["revised"] = Value::Object(revised);
                }
                value
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

    /// `captions`: the last lines of Them (and mine, when my captions are
    /// on), oldest first.
    pub fn captions(&self) -> Value {
        let Rows::Live(transcript) = &self.rows else {
            return json!([]);
        };
        let mut lines: Vec<Value> = transcript
            .rows()
            .iter()
            .rev()
            .filter(|row| !row.is_blank() && (row.source == Source::Them || self.my_captions))
            .take(CAPTION_LINES)
            .map(|row| json!({ "source": row.source.as_str(), "final": row.final_text, "partial": row.partial_text }))
            .collect();
        lines.reverse();
        Value::Array(lines)
    }
}

fn row_value(row: &Row) -> Value {
    let mut value = json!({
        "index": row.index + 1,
        "source": row.source.as_str(),
        "start_ms": row.start_ms,
        "final": row.final_text,
        "partial": row.partial_text,
        "open": row.is_open(),
    });
    if let Some(mark) = row.pause {
        value["pause"] = json!({ "paused_ms": mark.paused_ms, "resumed_ms": mark.resumed_ms });
    }
    value
}

fn pause_value(index: usize, mark: &PauseMark) -> Value {
    json!({
        "index": index,
        "source": "them",
        "start_ms": mark.at_ms,
        "final": "",
        "partial": "",
        "open": false,
        "pause": { "paused_ms": mark.paused_ms, "resumed_ms": mark.resumed_ms },
    })
}

/// One row per turn of a speaker: a speaker's segments in a row join. A
/// pause (sorted) is a marker row before what was said after it.
fn speaker_rows(info: &MeetingInfo, segments: &[recap_storage::Segment], pauses: &[PauseMark]) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    let mut last: Option<(&str, Source)> = None;
    let mut pauses = pauses.iter().peekable();
    for s in segments {
        while let Some(mark) = pauses.next_if(|p| p.at_ms <= s.start_ms) {
            rows.push(pause_value(rows.len() + 1, mark));
            last = None;
        }
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
    for mark in pauses {
        rows.push(pause_value(rows.len() + 1, mark));
    }
    rows
}
