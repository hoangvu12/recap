//! The refined transcript from the two batch transcripts: words merged by
//! time into turns, echoes of Them on the mic dropped.

use recap_engine::{BatchTranscript, Source, Turn, Word, refine};

fn word(text: &str, start_ms: u64, end_ms: u64, speaker: Option<&str>) -> Word {
    Word { text: text.into(), start_ms, end_ms, speaker: speaker.map(Into::into) }
}

/// `text` said from `start_ms`, a word every 300 ms.
fn said(text: &str, start_ms: u64, speaker: Option<&str>) -> Vec<Word> {
    text.split_whitespace()
        .enumerate()
        .map(|(i, w)| word(w, start_ms + i as u64 * 300, start_ms + i as u64 * 300 + 250, speaker))
        .collect()
}

fn transcript(parts: &[Vec<Word>]) -> BatchTranscript {
    BatchTranscript { words: parts.concat() }
}

fn summary(turns: &[Turn]) -> Vec<(Source, Option<&str>, u64, &str)> {
    turns.iter().map(|t| (t.source, t.speaker.as_deref(), t.start_ms, t.text.as_str())).collect()
}

#[test]
fn speakers_take_turns_in_time_order() {
    let them = transcript(&[
        said("Thanks, Tom. Quick status from design.", 4_000, Some("S1")),
        said("The payments API is ready.", 7_000, Some("S2")),
    ]);
    let me = transcript(&[said("Okay, let's get started.", 0, None), said("Great, thanks.", 10_000, None)]);
    let turns = refine::turns(&me, &them);
    assert_eq!(
        summary(&turns),
        vec![
            (Source::Me, Some("me"), 0, "Okay, let's get started."),
            (Source::Them, Some("S1"), 4_000, "Thanks, Tom. Quick status from design."),
            (Source::Them, Some("S2"), 7_000, "The payments API is ready."),
            (Source::Me, Some("me"), 10_000, "Great, thanks."),
        ]
    );
    assert_eq!(turns[1].end_ms, 4_000 + 5 * 300 + 250);
}

#[test]
fn a_speakers_sentences_in_a_row_are_one_turn() {
    let them = transcript(&[said("First sentence here.", 0, Some("S1")), said("And a second one.", 1_500, Some("S1"))]);
    let turns = refine::turns(&BatchTranscript::default(), &them);
    assert_eq!(summary(&turns), vec![(Source::Them, Some("S1"), 0, "First sentence here. And a second one.")]);
}

#[test]
fn my_words_inside_their_turn_split_it() {
    let them = transcript(&[
        said("We need one more week.", 0, Some("S1")),
        said("Then we launch on December first.", 4_000, Some("S1")),
    ]);
    let me = transcript(&[said("Mm, okay.", 2_500, None)]);
    let turns = refine::turns(&me, &them);
    assert_eq!(
        summary(&turns),
        vec![
            (Source::Them, Some("S1"), 0, "We need one more week."),
            (Source::Me, Some("me"), 2_500, "Mm, okay."),
            (Source::Them, Some("S1"), 4_000, "Then we launch on December first."),
        ]
    );
}

#[test]
fn their_words_heard_on_my_mic_are_dropped() {
    // Laptop speakers: the mic hears Them a little later, cut differently.
    let them = transcript(&[
        said("From the backend side, the payments API is ready.", 0, Some("S2")),
        said("I think we need one more week.", 3_000, Some("S2")),
    ]);
    let me = transcript(&[
        said("From the backend side, the payments API is ready. I think we need one more week.", 200, None),
        said("One more week pushes us to December first.", 6_000, None),
    ]);
    let turns = refine::turns(&me, &them);
    assert_eq!(
        summary(&turns),
        vec![
            (
                Source::Them,
                Some("S2"),
                0,
                "From the backend side, the payments API is ready. I think we need one more week."
            ),
            (Source::Me, Some("me"), 6_000, "One more week pushes us to December first."),
        ]
    );
}

#[test]
fn a_long_silence_starts_a_new_sentence_for_the_echo_check() {
    // No punctuation, but 5 s apart: two sentences, only the echo goes.
    let them = transcript(&[said("so that is the plan", 0, Some("S1"))]);
    let me = transcript(&[said("so that is the plan", 100, None), said("sounds good to me then", 6_000, None)]);
    let turns = refine::turns(&me, &them);
    assert_eq!(
        summary(&turns),
        vec![
            (Source::Them, Some("S1"), 0, "so that is the plan"),
            (Source::Me, Some("me"), 6_000, "sounds good to me then")
        ]
    );
}

#[test]
fn them_without_diarization_is_one_speaker() {
    let them = transcript(&[said("Hello there.", 0, None)]);
    let turns = refine::turns(&BatchTranscript::default(), &them);
    assert_eq!(summary(&turns), vec![(Source::Them, Some("S1"), 0, "Hello there.")]);
}

#[test]
fn nothing_heard_is_no_turns() {
    assert!(refine::turns(&BatchTranscript::default(), &BatchTranscript::default()).is_empty());
}
