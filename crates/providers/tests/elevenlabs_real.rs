//! Against the real ElevenLabs API: ignored unless asked for, and skipped
//! without `ELEVENLABS_API_KEY`.
//!
//! `cargo test -p recap-providers --test elevenlabs_real -- --ignored --nocapture`

use std::path::Path;
use std::time::{Duration, Instant};

use recap_engine::{AudioSource, BatchRequest, BatchStt, LiveEvent, LiveOptions, LiveStt, Source, WavSource};
use recap_providers::{
    Provider,
    elevenlabs::{Config, ElevenLabs},
};
use tokio::sync::mpsc;

fn key() -> Option<String> {
    std::env::var("ELEVENLABS_API_KEY").ok().filter(|k| !k.trim().is_empty())
}

fn wav() -> WavSource {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/meeting_tts.wav");
    WavSource::open(&path).expect("testdata/meeting_tts.wav")
}

#[tokio::test]
#[ignore = "real API: needs ELEVENLABS_API_KEY"]
async fn live_captions_of_the_test_meeting() {
    let Some(key) = key() else {
        eprintln!("ELEVENLABS_API_KEY is not set: skipped");
        return;
    };
    let stt = ElevenLabs::new("ElevenLabs", &key, Config::default());
    let (audio, audio_rx) = mpsc::unbounded_channel();
    let (events_tx, mut events) = mpsc::unbounded_channel();
    let options = LiveOptions { keyterms: vec!["Priya".into(), "David".into()], language: None };
    let session = tokio::spawn(stt.run(Source::Them, options, audio_rx, events_tx));

    // Both voices of the stereo file are in channel 1 (Them); in real time,
    // the way capture hands it out.
    let feeder = std::thread::spawn(move || {
        let mut source = wav();
        let started = Instant::now();
        let mut sent_ms = 0;
        while let Some(frame) = source.next_frame() {
            if frame.source != Source::Them {
                continue;
            }
            sent_ms += frame.duration_ms();
            let due = started + Duration::from_millis(sent_ms);
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            if audio.send(frame).is_err() {
                break;
            }
        }
    });

    let mut heard: Vec<LiveEvent> = Vec::new();
    let collect = async {
        while let Some(event) = events.recv().await {
            if event.is_final {
                eprintln!("[{:>6} {:>6}] {}", event.start_ms, event.end_ms, event.text);
            }
            heard.push(event);
        }
    };
    tokio::time::timeout(Duration::from_secs(200), collect).await.expect("the session ends after the audio");
    feeder.join().unwrap();
    session.await.unwrap().expect("no provider error");

    let partials = heard.iter().filter(|e| !e.is_final).count();
    let finals: Vec<&LiveEvent> = heard.iter().filter(|e| e.is_final).collect();
    eprintln!("{partials} partials, {} finals", finals.len());
    assert!(partials > 20, "partials stream in");
    assert!(finals.len() >= 5, "turns are committed");
    let text = finals.iter().map(|e| e.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
    assert!(text.contains("december 1"), "the launch date is heard: {text}");
    assert!(finals.windows(2).all(|w| w[0].start_ms <= w[1].start_ms), "finals in meeting order");
    assert!(finals.last().unwrap().end_ms > 100_000, "times are on the meeting's clock");
}

#[tokio::test]
#[ignore = "real API: needs ELEVENLABS_API_KEY"]
async fn the_test_button_and_a_wrong_key() {
    let Some(key) = key() else { return };
    let good = ElevenLabs::new("ElevenLabs", &key, Config::default());
    good.test().await.expect("the key works");
    let bad = ElevenLabs::new("ElevenLabs", "sk_wrong", Config::default());
    let error = bad.test().await.unwrap_err();
    assert_eq!(error.kind, recap_engine::ProviderErrorKind::Auth, "{error:?}");
}

#[tokio::test]
#[ignore = "real API: needs ELEVENLABS_API_KEY"]
async fn batch_transcript_with_speakers() {
    let Some(key) = key() else { return };
    let mut source = wav();
    let mut samples = Vec::new();
    while let Some(frame) = source.next_frame() {
        if frame.source == Source::Them {
            samples.extend(frame.samples);
        }
    }
    let stt = ElevenLabs::new("ElevenLabs", &key, Config::default());
    let transcript = stt
        .transcribe(BatchRequest { samples, diarize: true, keyterms: vec!["Priya".into()], language: None })
        .await
        .expect("a transcript");
    let text = transcript.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
    assert!(text.contains("december"), "{text}");
    let speakers: std::collections::BTreeSet<_> = transcript.words.iter().filter_map(|w| w.speaker.clone()).collect();
    eprintln!("{} words, speakers {speakers:?}", transcript.words.len());
    assert!(speakers.len() >= 2, "diarized");
}
