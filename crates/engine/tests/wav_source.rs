//! The WAV-file AudioSource: what tests and replays run on instead of devices.
//! It hands the engine the same thing live capture does: 16 kHz mono frames
//! for Me and Them on one aligned timeline, gaps zero-filled.

use std::path::{Path, PathBuf};
use std::time::Duration;

use recap_engine::{AudioFrame, AudioSource, SAMPLE_RATE, Source, WavSource};

const TEST_MEETING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/meeting_tts.wav");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("recap-wav-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// A WAV of `seconds`, each channel a tone (Hz) or silence (0.0).
fn tone_wav(name: &str, rate: u32, seconds: f64, tones: &[f64]) -> PathBuf {
    let path = scratch(name);
    let spec = hound::WavSpec {
        channels: tones.len() as u16,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for n in 0..(seconds * rate as f64) as usize {
        for &hz in tones {
            let t = n as f64 / rate as f64;
            let v = if hz == 0.0 { 0.0 } else { (t * hz * std::f64::consts::TAU).sin() * 0.5 };
            w.write_sample((v * i16::MAX as f64) as i16).unwrap();
        }
    }
    w.finalize().unwrap();
    path
}

fn read_all(mut source: impl AudioSource) -> Vec<AudioFrame> {
    let mut frames = Vec::new();
    while let Some(frame) = source.next_frame() {
        frames.push(frame);
    }
    frames
}

fn side(frames: &[AudioFrame], source: Source) -> Vec<&AudioFrame> {
    frames.iter().filter(|f| f.source == source).collect()
}

/// Mean absolute level of a stretch of one side, from `from_ms` to `to_ms`.
fn level(frames: &[AudioFrame], source: Source, from_ms: u64, to_ms: u64) -> f64 {
    let samples: Vec<i16> = side(frames, source)
        .iter()
        .filter(|f| f.start_ms >= from_ms && f.start_ms + f.duration_ms() <= to_ms)
        .flat_map(|f| f.samples.iter().copied())
        .collect();
    assert!(!samples.is_empty(), "{source:?} has audio between {from_ms} and {to_ms} ms");
    samples.iter().map(|s| s.unsigned_abs() as f64).sum::<f64>() / samples.len() as f64
}

/// Both sides cover the same timeline: frame for frame, same start, same size,
/// back to back from 0.
fn assert_aligned(frames: &[AudioFrame]) {
    let me = side(frames, Source::Me);
    let them = side(frames, Source::Them);
    assert_eq!(me.len(), them.len(), "as many Me frames as Them frames");
    let mut at = 0;
    for (m, t) in me.iter().zip(&them) {
        assert_eq!((m.start_ms, t.start_ms), (at, at), "frames back to back from 0");
        assert_eq!(m.samples.len(), t.samples.len());
        at += m.duration_ms();
    }
}

#[test]
fn a_stereo_wav_is_me_on_the_left_and_them_on_the_right_at_16_khz() {
    let path = tone_wav("stereo.wav", 48_000, 2.0, &[440.0, 0.0]);
    let frames = read_all(WavSource::open(&path).unwrap());

    assert_aligned(&frames);
    let total: usize = side(&frames, Source::Me).iter().map(|f| f.samples.len()).sum();
    let want = 2 * SAMPLE_RATE as usize;
    assert!(total.abs_diff(want) <= 1_600, "2 s at 16 kHz, give or take a frame: {total}");
    assert!(level(&frames, Source::Me, 200, 1_800) > 5_000.0, "Me carries the tone");
    assert!(level(&frames, Source::Them, 200, 1_800) < 10.0, "Them is silent");
}

#[test]
fn a_mono_wav_is_them_and_me_stays_silent_on_the_same_timeline() {
    let path = tone_wav("mono.wav", 44_100, 1.5, &[300.0]);
    let frames = read_all(WavSource::open(&path).unwrap());
    assert_aligned(&frames);
    assert!(level(&frames, Source::Them, 200, 1_300) > 5_000.0);
    assert!(level(&frames, Source::Me, 200, 1_300) < 1.0);
}

#[test]
fn a_side_that_starts_late_or_ends_early_is_zero_filled_to_stay_aligned() {
    // The mic joins 1 s in and drops out after 1 s of audio; the meeting audio
    // runs 3 s. Me is silence, tone, silence; Them is untouched throughout.
    let me = tone_wav("me_short.wav", 48_000, 1.0, &[440.0]);
    let them = tone_wav("them_long.wav", 16_000, 3.0, &[200.0]);
    let source = WavSource::sides(Some((&me, Duration::from_secs(1))), Some((&them, Duration::ZERO))).unwrap();
    let frames = read_all(source);

    assert_aligned(&frames);
    let end = side(&frames, Source::Them).last().map(|f| f.start_ms + f.duration_ms()).unwrap();
    assert!((2_900..=3_100).contains(&end), "the timeline runs as long as the longest side: {end}");
    assert!(level(&frames, Source::Me, 0, 900) < 1.0, "zeros before the mic joined");
    assert!(level(&frames, Source::Me, 1_200, 1_800) > 5_000.0, "the mic's audio at its place");
    assert!(level(&frames, Source::Me, 2_200, 2_900) < 1.0, "zeros after the mic dropped");
    assert!(level(&frames, Source::Them, 0, 2_900) > 5_000.0, "Them never gaps");
}

#[test]
fn the_test_meeting_replays_as_two_minutes_on_both_sides() {
    let frames = read_all(WavSource::open(Path::new(TEST_MEETING)).unwrap());
    assert_aligned(&frames);
    let end = side(&frames, Source::Them).last().map(|f| f.start_ms + f.duration_ms()).unwrap();
    assert!((118_000..=118_500).contains(&end), "118.3 s of meeting: {end}");
    // The file has the same mix on both channels: both sides hear it.
    assert!(level(&frames, Source::Me, 0, 110_000) > 300.0);
    assert!(level(&frames, Source::Them, 0, 110_000) > 300.0);
}
