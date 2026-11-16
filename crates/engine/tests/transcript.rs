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
fn a_rows_first_final_sets_where_it_began() {
    // Partials without word times guess the start; the final knows it.
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Them, 11_200, 11_900, "Thanks,", false));
    t.apply(&seg(1, Source::Them, 11_040, 23_640, "Thanks, Tom.", true));
    assert_eq!(t.row(0).unwrap().start_ms, 11_040);
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
fn a_side_heard_late_rejoins_its_turn_when_the_other_side_spoke_after_it() {
    // Each side's provider answers on its own: Me's next sentence (said at
    // 1.0 s) can arrive after Them's reply (said at 2.0 s) opened a row.
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Me, 0, 900, "Okay.", true));
    t.apply(&seg(2, Source::Them, 2_000, 2_400, "Thanks,", false));
    let change = t.apply(&seg(3, Source::Me, 1_000, 1_800, "Let's start.", true));
    assert_eq!((change.index, change.added), (0, false));
    assert_eq!(t.row(0).unwrap().final_text, "Okay. Let's start.");
    assert_eq!(t.len(), 2);
    // Said after Them's row started: a turn of its own.
    assert!(t.apply(&seg(4, Source::Me, 2_500, 3_000, "Go on.", true)).added);
}

#[test]
fn a_segment_taken_back_leaves_the_rest_of_its_row() {
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Me, 0, 900, "Okay.", true));
    t.apply(&seg(2, Source::Me, 1_000, 1_800, "We launch on", true));
    t.apply(&seg(3, Source::Me, 1_900, 2_300, "December", false));
    // Segment 2 was an echo: its words go, the row keeps the others.
    assert_eq!(t.forget(SegmentId(2)), Some(0));
    assert_eq!(t.row(0).unwrap().final_text, "Okay.");
    assert_eq!(t.row(0).unwrap().partial_text, "December");
    // A partial taken back closes the row.
    assert_eq!(t.forget(SegmentId(3)), Some(0));
    assert!(!t.row(0).unwrap().is_open());
    assert_eq!(t.row(0).unwrap().partial_text, "");
    assert_eq!(t.forget(SegmentId(9)), None);
}

#[test]
fn a_row_left_without_words_is_blank_and_takes_no_turn() {
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Me, 0, 900, "Echo.", true));
    t.forget(SegmentId(1));
    assert!(t.row(0).unwrap().is_blank());
    // The next Me words start a row of their own, at their own time.
    assert!(t.apply(&seg(2, Source::Me, 1_000, 1_400, "Mine.", true)).added);
}

#[test]
fn a_pause_marker_is_a_row_that_ends_the_turn() {
    let mut t = LiveTranscript::default();
    t.apply(&seg(1, Source::Them, 0, 900, "Before.", true));
    let mark = recap_engine::PauseMark { at_ms: 1_000, paused_ms: 10, resumed_ms: 20 };
    let change = t.mark_pause(mark);
    assert_eq!((change.index, change.added), (1, true));
    assert_eq!(t.row(1).unwrap().pause, Some(mark));
    assert!(t.row(1).unwrap().is_blank());
    // Them right after the pause: a new row, not the one before it.
    assert_eq!(t.apply(&seg(2, Source::Them, 1_000, 1_300, "After.", true)).index, 2);
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
