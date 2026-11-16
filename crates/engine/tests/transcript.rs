//! The live transcript: segments (partial and final) grouped into rows, one
//! row per turn, as the Notes window and the overlay show them.

use recap_engine::{LiveTranscript, Segment, SegmentId, Source};

fn seg(id: u64, source: Source, start_ms: u64, end_ms: u64, text: &str, is_final: bool) -> Segment {
    Segment { id: SegmentId(id), source, start_ms, end_ms, text: text.into(), is_final }
}

#[test]
fn a_partial_opens_a_row_and_its_final_settles_it() {
    let mut t = LiveTranscript::default();
    let change = t.apply(&seg(1, Source::Them, 1_000, 1_400, "Thanks,", false));
    assert_eq!((change.index, change.added), (0, true));
    assert_eq!(t.row(0).unwrap().partial_text, "Thanks,");
    assert_eq!(t.row(0).unwrap().final_text, "");
    assert!(t.row(0).unwrap().is_open());

    let change = t.apply(&seg(1, Source::Them, 1_000, 1_900, "Thanks, Tom.", true));
    assert_eq!((change.index, change.added), (0, false));
    let row = t.row(0).unwrap();
    assert_eq!(row.final_text, "Thanks, Tom.");
    assert_eq!(row.partial_text, "");
    assert!(!row.is_open());
    assert_eq!((row.start_ms, row.end_ms), (1_000, 1_900));
}

#[test]
fn the_next_sentence_of_the_same_side_continues_the_row() {
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Them, 0, 900, "Thanks, Tom.", true));
    let change = t.apply(&seg(2, Source::Them, 1_100, 1_500, "Quick", false));
    assert_eq!((change.index, change.added), (0, false));
    assert_eq!(t.row(0).unwrap().final_text, "Thanks, Tom.");
    assert_eq!(t.row(0).unwrap().partial_text, "Quick");
    t.apply(&seg(2, Source::Them, 1_100, 2_000, "Quick status.", true));
    assert_eq!(t.row(0).unwrap().final_text, "Thanks, Tom. Quick status.");
    assert_eq!(t.len(), 1);
}

#[test]
fn a_pause_or_the_other_side_starts_a_new_row() {
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Them, 0, 900, "One.", true));
    // The other side speaks.
    assert!(t.apply(&seg(2, Source::Me, 1_000, 1_500, "Two.", true)).added);
    // Them again, right after Me: a new turn.
    assert!(t.apply(&seg(3, Source::Them, 1_600, 2_000, "Three.", true)).added);
    // Them after a long gap: a new turn too (another person on the call).
    assert!(t.apply(&seg(4, Source::Them, 5_000, 5_500, "Four.", true)).added);
    let texts: Vec<_> = (0..t.len()).map(|i| t.row(i).unwrap().final_text.clone()).collect();
    assert_eq!(texts, ["One.", "Two.", "Three.", "Four."]);
}

#[test]
fn a_segment_stays_in_its_row_while_the_other_side_talks_over_it() {
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Them, 0, 300, "So we", false));
    t.apply(&seg(2, Source::Me, 200, 400, "Mm", false));
    let change = t.apply(&seg(1, Source::Them, 0, 900, "So we launch.", true));
    assert_eq!(change.index, 0);
    assert_eq!(t.row(0).unwrap().final_text, "So we launch.");
    assert_eq!(t.row(1).unwrap().partial_text, "Mm");
    assert_eq!(t.open_rows().collect::<Vec<_>>(), vec![1]);
}

#[test]
fn finishing_turns_what_is_still_partial_into_final_text() {
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Them, 0, 900, "One.", true));
    t.apply(&seg(2, Source::Them, 1_000, 1_300, "Two and", false));
    let changed = t.finish();
    assert_eq!(changed, vec![0]);
    assert_eq!(t.row(0).unwrap().final_text, "One. Two and");
    assert!(!t.row(0).unwrap().is_open());
}
