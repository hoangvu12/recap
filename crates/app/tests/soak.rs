//! The soak run's command line and how it judges its samples.

use recap::{
    live::Audio,
    soak::{Limits, Sample, Soak, verdict},
};

const MIB: u64 = 1024 * 1024;

/// An hour of samples, a minute apart: private MiB and handles by minute.
fn run(private_mib: impl Fn(u32) -> f64, handles: impl Fn(u32) -> u32) -> Vec<Sample> {
    (0..=60)
        .map(|minute| Sample {
            minute,
            private_bytes: (private_mib(minute) * MIB as f64) as u64,
            handles: handles(minute),
            gdi_objects: 40,
            user_objects: 30,
        })
        .collect()
}

#[test]
fn soak_takes_minutes_and_loops_the_test_meeting_with_scripted_captions() {
    let args = |s: &str| s.split(' ').map(str::to_owned).collect::<Vec<_>>();
    let soak = Soak::from_args(args("--soak 45")).unwrap();
    assert_eq!(soak.minutes, 45);
    assert!(soak.wav.ends_with("testdata/meeting_tts.wav") && soak.wav.is_file());
    let options = soak.start_options();
    assert!(options.scripted, "no real provider");
    assert_eq!(options.audio, Audio::Looped(soak.wav.clone()));

    assert_eq!(Soak::from_args(args("--soak 1 --replay other.wav")).unwrap().wav, std::path::Path::new("other.wav"));
    assert_eq!(Soak::from_args(args("--scripted")), None);
    assert_eq!(Soak::from_args(args("--soak")), None);
    assert_eq!(Soak::from_args(args("--soak 0")), None);
}

#[test]
fn a_flat_run_passes_despite_noise_and_start_up() {
    // Start-up climbs for 3 minutes, then it wobbles by a MiB and a few handles.
    let samples = run(
        |m| if m < 3 { 60. + m as f64 * 10. } else { 90. + (m % 3) as f64 },
        |m| if m < 3 { 300 + m * 50 } else { 450 + m % 5 },
    );
    let verdict = verdict(&samples, &Limits::default());
    assert!(verdict.ok, "{}", verdict.report());
    assert!(verdict.report().starts_with("PASS"));
    assert_eq!(verdict.lines.len(), 4, "private bytes, handles, GDI, USER");
}

#[test]
fn memory_that_keeps_growing_fails() {
    // 0.5 MiB a minute: 30 MiB an hour.
    let samples = run(|m| 90. + m as f64 * 0.5, |_| 450);
    let verdict = verdict(&samples, &Limits::default());
    assert!(!verdict.ok);
    assert!(verdict.lines[0].contains("GREW"), "{}", verdict.report());
    assert!(!verdict.lines[1].contains("GREW"));
}

#[test]
fn handles_that_keep_growing_fail() {
    let samples = run(|_| 90., |m| 450 + m * 3);
    let verdict = verdict(&samples, &Limits::default());
    assert!(!verdict.ok);
    assert!(verdict.lines[1].contains("GREW"), "{}", verdict.report());
}

#[test]
fn growth_that_levels_off_is_not_sustained() {
    // Grows for the first 20 minutes, then flat for the 40 after.
    let samples = run(|m| 90. + m.min(20) as f64 * 0.5, |_| 450);
    let verdict = verdict(&samples, &Limits::default());
    assert!(verdict.ok, "{}", verdict.report());
}

#[test]
fn a_run_too_short_to_judge_says_so() {
    let samples: Vec<Sample> = run(|m| 90. + m as f64, |_| 450).into_iter().take(7).collect();
    let verdict = verdict(&samples, &Limits::default());
    assert!(verdict.ok);
    assert!(verdict.lines[0].contains("too short"));
}

#[test]
fn this_process_can_be_sampled() {
    let sample = Sample::now(0);
    if cfg!(windows) {
        assert!(sample.private_bytes > 0 && sample.handles > 0, "{sample:?}");
    }
    assert_eq!(sample.csv().split(',').count(), Sample::HEADER.split(',').count());
}
