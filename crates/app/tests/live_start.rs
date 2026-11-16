//! How Start begins a meeting: the dev flags.

use recap::live::{Audio, StartOptions, capture_options};
use recap_engine::CaptureOptions;
use recap_storage::{MeetingAudio, Settings};

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
    let devices = CaptureOptions::default();
    assert!(options(&["--replay", wav]).source(&devices).is_ok_and(|(_, note)| note.is_none()));
    let error = options(&["--replay", "no/such.wav"]).source(&devices).err().unwrap();
    assert!(error.contains("no/such.wav"), "{error}");
}

#[test]
fn the_audio_settings_choose_the_capture_devices() {
    assert_eq!(capture_options(&Settings::default()), CaptureOptions::default());
    let chosen = Settings {
        microphone: "{0.0.1.00000000}.{headset}".into(),
        meeting_audio: MeetingAudio::App("ms-teams.exe".into()),
        ..Settings::default()
    };
    assert_eq!(
        capture_options(&chosen),
        CaptureOptions {
            microphone: Some("{0.0.1.00000000}.{headset}".into()),
            meeting_app: Some("ms-teams.exe".into()),
        }
    );
}
