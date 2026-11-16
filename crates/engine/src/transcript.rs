use std::collections::HashMap;

use crate::{PauseMark, Segment, SegmentId, Source};

/// One row of the live transcript: a turn by one side, its final text and the
/// partial still being recognised after it; or a pause marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Its place in the transcript, from 0.
    pub index: usize,
    pub source: Source,
    pub start_ms: u64,
    pub end_ms: u64,
    pub final_text: String,
    pub partial_text: String,
    /// A pause marker row (no text): the meeting paused here.
    pub pause: Option<PauseMark>,
    open: Option<SegmentId>,
    /// The final segments `final_text` is made of, so one can be taken back.
    pieces: Vec<(SegmentId, String)>,
}

impl Row {
    /// Whether a segment in it is still partial.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether it holds no words (a dropped echo's row, a pause marker).
    pub fn is_blank(&self) -> bool {
        self.final_text.is_empty() && self.partial_text.is_empty()
    }
}

/// Which row a segment changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowChange {
    pub index: usize,
    /// A new row (the transcript grew).
    pub added: bool,
}

/// Segments grouped into rows: the next segment of the same side joins its
/// last turn when it follows it closely; the other side speaking, or a pause,
/// starts a new row. Rows are only ever added at the end, and placed by
/// meeting time, not by when a side's text arrived.
#[derive(Clone, Debug)]
pub struct LiveTranscript {
    rows: Vec<Row>,
    /// Segments still partial: their rows.
    by_segment: HashMap<SegmentId, usize>,
    /// Final segments: their rows (to take one back).
    settled: HashMap<SegmentId, usize>,
    join_gap_ms: u64,
}

impl Default for LiveTranscript {
    fn default() -> Self {
        Self::with_join_gap(400)
    }
}

fn joined(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_owned(),
        (_, true) => a.to_owned(),
        _ => format!("{a} {b}"),
    }
}

impl LiveTranscript {
    /// A gap up to `join_gap_ms` between a row's end and the same side's next
    /// segment keeps them one row.
    pub fn with_join_gap(join_gap_ms: u64) -> Self {
        Self { rows: Vec::new(), by_segment: HashMap::new(), settled: HashMap::new(), join_gap_ms }
    }

    fn push(&mut self, source: Source, start_ms: u64, end_ms: u64, pause: Option<PauseMark>) -> usize {
        let index = self.rows.len();
        self.rows.push(Row {
            index,
            source,
            start_ms,
            end_ms,
            final_text: String::new(),
            partial_text: String::new(),
            pause,
            open: None,
            pieces: Vec::new(),
        });
        index
    }

    pub fn apply(&mut self, segment: &Segment) -> RowChange {
        let (index, added) = match self.by_segment.get(&segment.id) {
            Some(&index) => (index, false),
            None => {
                // The side's latest turn, if this continues it: close after
                // it, and nothing of the other side said in between (rows
                // after it that started later only arrived first: each
                // side's provider answers on its own).
                let turn = self
                    .rows
                    .iter()
                    .rposition(|r| r.source == segment.source && r.pause.is_none())
                    .filter(|&i| {
                        let row = &self.rows[i];
                        !row.is_open()
                            && !row.final_text.is_empty()
                            && segment.start_ms <= row.end_ms + self.join_gap_ms
                            && self.rows[i + 1..].iter().all(|later| later.start_ms > segment.start_ms)
                    });
                match turn {
                    Some(index) => (index, false),
                    None => (self.push(segment.source, segment.start_ms, segment.end_ms, None), true),
                }
            }
        };
        let row = &mut self.rows[index];
        row.end_ms = row.end_ms.max(segment.end_ms);
        if segment.is_final {
            // A row's first final has the words' own times; its partials
            // only guessed where it began (a provider's partials may carry
            // no times).
            if row.pieces.is_empty() {
                row.start_ms = segment.start_ms;
            }
            row.final_text = joined(&row.final_text, &segment.text);
            row.pieces.push((segment.id, segment.text.clone()));
            row.partial_text.clear();
            row.open = None;
            self.by_segment.remove(&segment.id);
            self.settled.insert(segment.id, index);
        } else {
            row.partial_text = segment.text.clone();
            row.open = Some(segment.id);
            self.by_segment.insert(segment.id, index);
        }
        RowChange { index, added }
    }

    /// Take back segment `id`, partial or final (an echo); the row it was in.
    pub fn forget(&mut self, id: SegmentId) -> Option<usize> {
        if let Some(index) = self.by_segment.remove(&id) {
            let row = &mut self.rows[index];
            row.partial_text.clear();
            row.open = None;
            return Some(index);
        }
        let index = self.settled.remove(&id)?;
        let row = &mut self.rows[index];
        row.pieces.retain(|(piece, _)| *piece != id);
        row.final_text = row.pieces.iter().fold(String::new(), |text, (_, piece)| joined(&text, piece));
        Some(index)
    }

    /// A pause marker row at the end.
    pub fn mark_pause(&mut self, mark: PauseMark) -> RowChange {
        let index = self.push(Source::Them, mark.at_ms, mark.at_ms, Some(mark));
        RowChange { index, added: true }
    }

    /// Make what is still partial final (the meeting stopped); the rows changed.
    pub fn finish(&mut self) -> Vec<usize> {
        let mut changed = Vec::new();
        for row in self.rows.iter_mut().filter(|r| r.is_open()) {
            row.final_text = joined(&row.final_text, &row.partial_text);
            if let Some(id) = row.open.take() {
                row.pieces.push((id, std::mem::take(&mut row.partial_text)));
            }
            row.partial_text.clear();
            changed.push(row.index);
        }
        self.by_segment.clear();
        changed
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The rows with a partial in them.
    pub fn open_rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.rows.iter().filter(|r| r.is_open()).map(|r| r.index)
    }
}
