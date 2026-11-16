//! How Start begins a meeting: the dev flags.

use recap::live::{Audio, StartOptions};

fn options(args: &[&str]) -> StartOptions {
    StartOptions::from_args(args.iter().map(|a| a.to_string()))
}

#[test]
fn by_default_start_captures_and_uses_the_live_job() {
    assert_eq!(options(&[]), StartOptions { audio: Audio::Capture, scripted: false });
    assert_eq!(options(&["--inspect"]), StartOptions::default());
}

#[test]
fn replay_reads_a_wav_and_scripted_replaces_the_provider() {
    let wav = "testdata/meeting_tts.wav";
    assert_eq!(options(&["--replay", wav]).audio, Audio::Replay(wav.into()));
    assert_eq!(options(&["--replay", wav, "--scripted"]), StartOptions { audio: Audio::Replay(wav.into()), scripted: true });
    // The test meeting without audio, as the app ran before capture.
    assert_eq!(options(&["--scripted"]), StartOptions { audio: Audio::Silence, scripted: true });
}

#[test]
fn a_replay_source_opens_the_wav_and_a_missing_one_says_so() {
    let wav = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/meeting_tts.wav");
    assert!(options(&["--replay", wav]).source().is_ok());
    let error = options(&["--replay", "no/such.wav"]).source().err().unwrap();
    assert!(error.contains("no/such.wav"), "{error}");
}
