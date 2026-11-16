//! The recording: stereo Ogg/Opus (ch0 Me, ch1 Them), written as it goes so a
//! crash or a kill loses at most a few seconds.

use std::path::PathBuf;

use recap_engine::{AudioFrame, Recorder, Recording, SAMPLE_RATE, Source};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("recap-rec-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// 100 ms of a tone (Hz) on one side, starting at `start_ms`.
fn tone(source: Source, start_ms: u64, hz: f64) -> AudioFrame {
    let n = SAMPLE_RATE as usize / 10;
    let first = start_ms as usize * SAMPLE_RATE as usize / 1000;
    let samples = (0..n)
        .map(|i| {
            let t = (first + i) as f64 / SAMPLE_RATE as f64;
            ((t * hz * std::f64::consts::TAU).sin() * 0.4 * i16::MAX as f64) as i16
        })
        .collect();
    AudioFrame { source, start_ms, samples }
}

/// `seconds` of a meeting: Me speaks a 300 Hz tone, Them is silent.
fn record(recorder: &mut Recorder, from_s: u64, seconds: u64) {
    for i in from_s * 10..(from_s + seconds) * 10 {
        recorder.write(&tone(Source::Me, i * 100, 300.0)).unwrap();
        recorder.write(&AudioFrame::silence(Source::Them, i * 100, 100)).unwrap();
    }
}

fn mean(samples: &[i16]) -> f64 {
    samples.iter().map(|s| s.unsigned_abs() as f64).sum::<f64>() / samples.len().max(1) as f64
}

#[test]
fn a_recording_plays_back_with_me_left_and_them_right() {
    let path = scratch("whole.ogg");
    let mut recorder = Recorder::create(&path).unwrap();
    record(&mut recorder, 0, 10);
    recorder.finish().unwrap();

    let rec = Recording::read(&path).unwrap();
    assert_eq!(rec.me.len(), rec.them.len());
    let secs = rec.me.len() as f64 / SAMPLE_RATE as f64;
    assert!((secs - 10.0).abs() < 0.1, "10 s recorded, {secs} s played back");
    assert!(mean(&rec.me[16_000..144_000]) > 5_000.0, "Me is on channel 0");
    assert!(mean(&rec.them[16_000..144_000]) < 200.0, "Them is on channel 1");
    // About 32 kbps: 10 s is about 40 KB.
    let size = std::fs::metadata(&path).unwrap().len();
    assert!((20_000..80_000).contains(&size), "{size} bytes");
}

#[test]
fn a_recording_cut_off_mid_write_still_decodes_up_to_the_cut() {
    let path = scratch("killed.ogg");
    let mut recorder = Recorder::create(&path).unwrap();
    record(&mut recorder, 0, 12);
    // Killed here: no finish(). What is on disk is what a crash leaves.
    let on_disk = std::fs::read(&path).unwrap();
    std::mem::forget(recorder);

    // The writer flushes every few seconds: most of the 12 s is already there.
    let crashed = scratch("crashed.ogg");
    std::fs::write(&crashed, &on_disk).unwrap();
    let rec = Recording::read(&crashed).unwrap();
    let secs = rec.me.len() as f64 / SAMPLE_RATE as f64;
    assert!(secs >= 7.0, "at most a few seconds lost: {secs} s of 12");

    // And a file cut in the middle of a page (power loss mid-write) decodes
    // everything before the cut.
    let cut = scratch("cut.ogg");
    std::fs::write(&cut, &on_disk[..on_disk.len() * 2 / 3 + 7]).unwrap();
    let rec = Recording::read(&cut).unwrap();
    let cut_secs = rec.me.len() as f64 / SAMPLE_RATE as f64;
    assert!(cut_secs >= secs * 0.5, "{cut_secs} s survive the cut");
    assert!(mean(&rec.me[16_000..]) > 5_000.0, "and it is the real audio");
}
