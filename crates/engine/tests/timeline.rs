//! The timeline live capture and WAV replay share: stamped packets in, aligned
//! 16 kHz mono frames out, with gaps zero-filled and late audio dropped.

use std::time::Duration;

use recap_engine::{AudioFrame, Packet, Source, Timeline};

const RATE: u32 = 48_000;

/// 10 ms of a constant level, stereo, stamped at `at_ms`.
fn packet(at_ms: u64, value: f32) -> Packet {
    Packet {
        at: Duration::from_millis(at_ms),
        rate: RATE,
        channels: 2,
        samples: vec![value; (RATE / 100 * 2) as usize],
    }
}

/// Packets every 10 ms from `from_ms` to `to_ms`.
fn run(t: &mut Timeline, source: Source, from_ms: u64, to_ms: u64, value: f32) {
    for at in (from_ms..to_ms).step_by(10) {
        t.push(source, packet(at, value));
    }
}

fn samples(frames: &[AudioFrame], source: Source) -> Vec<i16> {
    frames.iter().filter(|f| f.source == source).flat_map(|f| f.samples.iter().copied()).collect()
}

/// The level of `source` around `at_ms` (a 40 ms window), at 16 kHz.
fn at(all: &[i16], at_ms: u64) -> f64 {
    let i = (at_ms * 16) as usize;
    let window = &all[i - 320..i + 320];
    window.iter().map(|s| s.unsigned_abs() as f64).sum::<f64>() / window.len() as f64
}

#[test]
fn a_dropout_on_one_side_is_zero_filled_and_the_other_side_is_untouched() {
    let mut t = Timeline::new();
    run(&mut t, Source::Them, 0, 3_000, 0.5);
    // The mic delivers nothing from 1.0 s to 2.0 s (device lost, reopening).
    run(&mut t, Source::Me, 0, 1_000, 0.25);
    run(&mut t, Source::Me, 2_000, 3_000, 0.25);
    let mut frames = t.frames(Duration::from_millis(3_000), Duration::ZERO);
    frames.extend(t.finish());

    let me = samples(&frames, Source::Me);
    let them = samples(&frames, Source::Them);
    assert_eq!(me.len(), them.len(), "both sides the same length");
    assert!((at(&me, 500) - 0.25 * 32767.0).abs() < 500.0, "before the gap");
    assert!(at(&me, 1_500) < 1.0, "the gap is zeros");
    assert!((at(&me, 2_500) - 0.25 * 32767.0).abs() < 500.0, "after the gap, at its own time");
    assert!((at(&them, 1_500) - 0.5 * 32767.0).abs() < 500.0, "Them carries on");
}

#[test]
fn a_side_that_stalls_is_zero_filled_once_its_wait_is_over_and_late_audio_is_dropped() {
    let mut t = Timeline::new();
    run(&mut t, Source::Them, 0, 2_000, 0.5);
    // Me has said nothing yet; within the wait nothing is handed out.
    assert!(t.frames(Duration::from_millis(2_000), Duration::from_secs(5)).is_empty());
    // Past the wait, Me is zero-filled so Them is not held up.
    let early = t.frames(Duration::from_millis(2_000), Duration::from_millis(300));
    let me_early = samples(&early, Source::Me);
    assert!(me_early.len() >= 16 * 1_500, "Them flows: {} samples", me_early.len());
    assert!(me_early.iter().all(|s| *s == 0));
    let handed_out = me_early.len();

    // Me's audio for that stretch turns up late: it is dropped, not shifted.
    run(&mut t, Source::Me, 0, 2_000, 0.25);
    run(&mut t, Source::Them, 2_000, 2_500, 0.5);
    run(&mut t, Source::Me, 2_000, 2_500, 0.25);
    let mut late = t.frames(Duration::from_millis(2_500), Duration::ZERO);
    late.extend(t.finish());
    let me = [me_early, samples(&late, Source::Me)].concat();
    let them = [samples(&early, Source::Them), samples(&late, Source::Them)].concat();
    assert_eq!(me.len(), them.len());
    assert!(me.len() <= 16 * 2_600, "nothing was shifted later: {}", me.len());
    assert!(me[..handed_out].iter().all(|s| *s == 0));
    assert!((at(&me, 2_300) - 0.25 * 32767.0).abs() < 500.0, "new audio lands at its own time");
}

#[test]
fn small_clock_jitter_neither_gaps_nor_overlaps() {
    // Packets stamped a few ms early or late (QPC jitter) play back seamless.
    let mut t = Timeline::new();
    for (i, at_ms) in (0..2_000u64).step_by(10).enumerate() {
        let jitter = [0i64, 3, -4, 2, -1][i % 5];
        let mut p = packet(at_ms, 0.5);
        p.at = Duration::from_millis((at_ms as i64 + jitter).max(0) as u64);
        t.push(Source::Them, p);
        t.push(Source::Me, packet(at_ms, 0.5));
    }
    let mut frames = t.frames(Duration::from_millis(2_000), Duration::ZERO);
    frames.extend(t.finish());
    let them = samples(&frames, Source::Them);
    // No zeros in the middle: every 10 ms window has the level.
    for ms in (100..1_900).step_by(10) {
        assert!(at(&them, ms) > 0.4 * 32767.0, "no gap at {ms} ms");
    }
}
