use std::collections::HashMap;

use crate::{Segment, SegmentId, Source};

/// One row of the live transcript: a turn by one side, its final text and the
/// partial still being recognised after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Its place in the transcript, from 0.
    pub index: usize,
    pub source: Source,
    pub start_ms: u64,
    pub end_ms: u64,
    pub final_text: String,
    pub partial_text: String,
    open: Option<SegmentId>,
}

impl Row {
    /// Whether a segment in it is still partial.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

/// Which row a segment changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowChange {
    pub index: usize,
    /// A new row (the transcript grew).
    pub added: bool,
}

/// Segments grouped into rows: the next segment of the same side joins the
/// last row when it follows it closely; the other side speaking, or a pause,
/// starts a new row.
#[derive(Clone, Debug)]
pub struct LiveTranscript {
    rows: Vec<Row>,
    by_segment: HashMap<SegmentId, usize>,
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
        Self { rows: Vec::new(), by_segment: HashMap::new(), join_gap_ms }
    }

    pub fn apply(&mut self, segment: &Segment) -> RowChange {
        let (index, added) = match self.by_segment.get(&segment.id) {
            Some(&index) => (index, false),
            None => {
                let joins = self.rows.last().is_some_and(|last| {
                    last.source == segment.source
                        && !last.is_open()
                        && segment.start_ms <= last.end_ms + self.join_gap_ms
                });
                if joins {
                    (self.rows.len() - 1, false)
                } else {
                    let index = self.rows.len();
                    self.rows.push(Row {
                        index,
                        source: segment.source,
                        start_ms: segment.start_ms,
                        end_ms: segment.end_ms,
                        final_text: String::new(),
                        partial_text: String::new(),
                        open: None,
                    });
                    (index, true)
                }
            }
        };
        let row = &mut self.rows[index];
        row.end_ms = row.end_ms.max(segment.end_ms);
        if segment.is_final {
            row.final_text = joined(&row.final_text, &segment.text);
            row.partial_text.clear();
            row.open = None;
            self.by_segment.remove(&segment.id);
        } else {
            row.partial_text = segment.text.clone();
            row.open = Some(segment.id);
            self.by_segment.insert(segment.id, index);
        }
        RowChange { index, added }
    }

    /// Make what is still partial final (the meeting stopped); the rows changed.
    pub fn finish(&mut self) -> Vec<usize> {
        let mut changed = Vec::new();
        for row in self.rows.iter_mut().filter(|r| r.is_open()) {
            row.final_text = joined(&row.final_text, &row.partial_text);
            row.partial_text.clear();
            row.open = None;
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
