//! Echo dedup: the mic hears the meeting audio from the laptop's speakers, so
//! the other side's words come back as Me. A Me segment that says what Them
//! said at about the same time is an echo.

use recap_engine::echo::{ECHO_SIMILARITY, ECHO_WINDOW_MS, Spoken, is_echo, similarity};

fn spoken(start_ms: u64, end_ms: u64, text: &str) -> Spoken<'_> {
    Spoken { start_ms, end_ms, text }
}

const LINE: &str = "Then I'd suggest we launch on December first, with caching in place.";

#[test]
fn the_rule_is_one_point_seven_five_seconds_and_eighty_five_percent() {
    assert_eq!(ECHO_WINDOW_MS, 1_750);
    assert!((ECHO_SIMILARITY - 0.85).abs() < f64::EPSILON);
}

#[test]
fn the_same_words_at_the_same_time_are_an_echo() {
    assert!(is_echo(spoken(10_000, 14_000, LINE), &[spoken(9_900, 13_900, LINE)]));
}

#[test]
fn case_and_punctuation_do_not_matter() {
    let mic = "then id suggest we launch on december first with caching in place";
    assert!(is_echo(spoken(10_000, 14_000, mic), &[spoken(10_000, 14_000, LINE)]));
}

#[test]
fn the_same_words_said_later_are_not_an_echo() {
    // Saying back what Them said, seconds later, is the user speaking.
    assert!(!is_echo(spoken(20_000, 24_000, LINE), &[spoken(10_000, 14_000, LINE)]));
}

#[test]
fn the_window_is_measured_between_the_two_segments() {
    let me = spoken(5_000, 6_000, "we launch on December first");
    // Them ended 1 750 ms before Me started: still an echo.
    assert!(is_echo(me, &[spoken(2_000, 3_250, "we launch on December first")]));
    // 1 751 ms: not.
    assert!(!is_echo(me, &[spoken(2_000, 3_249, "we launch on December first")]));
    // After Me, the same on the other side.
    assert!(is_echo(me, &[spoken(7_750, 9_000, "we launch on December first")]));
    assert!(!is_echo(me, &[spoken(7_751, 9_000, "we launch on December first")]));
}

#[test]
fn a_word_misheard_by_the_mic_is_still_an_echo() {
    // 10 words, 9 the same: 0.9.
    let them = "the payments API is ready but load testing showed slow responses";
    let mic = "the payment API is ready but load testing showed slow responses";
    assert!(similarity(mic, them) >= 0.85, "{}", similarity(mic, them));
    assert!(is_echo(spoken(0, 4_000, mic), &[spoken(0, 4_000, them)]));
}

#[test]
fn two_words_in_ten_misheard_is_not() {
    let them = "one two three four five six seven eight nine ten";
    let mic = "one two three four five six seven eight nein tan";
    assert!((similarity(mic, them) - 0.8).abs() < 1e-9, "{}", similarity(mic, them));
    assert!(!is_echo(spoken(0, 4_000, mic), &[spoken(0, 4_000, them)]));
}

#[test]
fn the_mic_hearing_part_of_a_longer_turn_is_an_echo() {
    // The two providers cut turns differently.
    let mic = "we launch on December first, with caching";
    assert!(is_echo(spoken(11_000, 13_000, mic), &[spoken(9_000, 14_000, LINE)]));
}

#[test]
fn a_turn_split_across_two_them_segments_still_matches() {
    let mic = "Yes, I can finish dark mode by Friday, so that's not a blocker.";
    let them = [spoken(40_000, 42_000, "Yes, I can finish dark mode"), spoken(42_100, 44_000, "by Friday, so that's not a blocker.")];
    assert!(is_echo(spoken(40_000, 44_000, mic), &them));
}

#[test]
fn different_words_at_the_same_time_are_not_an_echo() {
    let mic = "Sorry, can you repeat the date for the launch?";
    assert!(!is_echo(spoken(10_000, 14_000, mic), &[spoken(10_000, 14_000, LINE)]));
}

#[test]
fn a_short_reply_is_not_lost_in_a_long_turn() {
    // "Yes" is in many sentences; a short Me line must match the whole of
    // what Them said, not just appear in it.
    let them = "Yes, I can finish dark mode by Friday, so that's not a blocker.";
    assert!(!is_echo(spoken(40_000, 40_500, "Yes."), &[spoken(40_000, 44_000, them)]));
    assert!(is_echo(spoken(40_000, 40_500, "Yes."), &[spoken(40_000, 40_400, "Yes.")]));
}

#[test]
fn nothing_from_them_means_no_echo() {
    assert!(!is_echo(spoken(0, 1_000, LINE), &[]));
    assert!(!is_echo(spoken(0, 1_000, ""), &[spoken(0, 1_000, LINE)]));
}
