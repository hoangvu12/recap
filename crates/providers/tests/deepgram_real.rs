//! Against the real Deepgram API: ignored unless asked for, and skipped
//! without `DEEPGRAM_API_KEY`. (Not run yet: there was no Deepgram key when
//! the adapter was written; the fakes replay Deepgram's documented messages.)
//!
//! `cargo test -p recap-providers --test deepgram_real -- --ignored --nocapture`

use std::path::Path;
use std::time::{Duration, Instant};

use recap_engine::{
    AudioSource, BatchRequest, BatchStt, LiveEvent, LiveOptions, LiveStt, ProviderErrorKind, Source, WavSource,
};
use recap_providers::{
    Provider,
    deepgram::{Config, Deepgram},
};
use tokio::sync::mpsc;

fn key() -> Option<String> {
    std::env::var("DEEPGRAM_API_KEY").ok().filter(|k| !k.trim().is_empty())
}

fn wav() -> WavSource {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/meeting_tts.wav");
    WavSource::open(&path).expect("testdata/meeting_tts.wav")
}

#[tokio::test]
#[ignore = "real API: needs DEEPGRAM_API_KEY"]
async fn live_captions_of_the_test_meeting() {
    let Some(key) = key() else {
        eprintln!("DEEPGRAM_API_KEY is not set: skipped");
        return;
    };
    let stt = Deepgram::new("Deepgram", key.trim(), Config::default());
    let (audio, audio_rx) = mpsc::unbounded_channel();
    let (events_tx, mut events) = mpsc::unbounded_channel();
    let options = LiveOptions { keyterms: vec!["Priya".into(), "David".into()], language: None };
    let session = tokio::spawn(stt.run(Source::Them, options, audio_rx, events_tx));

    // Channel 1 (Them) holds every voice; in real time, as capture would.
    let feeder = std::thread::spawn(move || {
        let mut source = wav();
        let started = Instant::now();
        let mut sent_ms = 0;
        while let Some(frame) = source.next_frame() {
            if frame.source != Source::Them {
                continue;
            }
            sent_ms += frame.duration_ms();
            std::thread::sleep((started + Duration::from_millis(sent_ms)).saturating_duration_since(Instant::now()));
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
    assert!(partials > 20, "interim results stream in");
    assert!(finals.len() >= 5, "utterances are final");
    let text = finals.iter().map(|e| e.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
    assert!(text.contains("december"), "the launch date is heard: {text}");
    assert!(finals.windows(2).all(|w| w[0].start_ms <= w[1].start_ms), "finals in meeting order");
    assert!(finals.last().unwrap().end_ms > 100_000, "times are on the meeting's clock");
}

#[tokio::test]
#[ignore = "real API: needs DEEPGRAM_API_KEY"]
async fn the_test_button_and_a_wrong_key() {
    let Some(key) = key() else { return };
    Deepgram::new("Deepgram", key.trim(), Config::default()).test().await.expect("the key works");
    let error = Deepgram::new("Deepgram", "dg_wrong", Config::default()).test().await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Auth, "{error:?}");
}

#[tokio::test]
#[ignore = "real API: needs DEEPGRAM_API_KEY"]
async fn batch_transcript_with_speakers() {
    let Some(key) = key() else { return };
    let mut source = wav();
    let mut samples = Vec::new();
    while let Some(frame) = source.next_frame() {
        if frame.source == Source::Them {
            samples.extend(frame.samples);
        }
    }
    let stt = Deepgram::new("Deepgram", key.trim(), Config::default());
    let started = Instant::now();
    let transcript = stt
        .transcribe(BatchRequest { samples, diarize: true, keyterms: vec!["Priya".into()], language: None })
        .await
        .expect("a transcript");
    let text = transcript.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
    assert!(text.contains("december"), "{text}");
    let speakers: std::collections::BTreeSet<_> = transcript.words.iter().filter_map(|w| w.speaker.clone()).collect();
    eprintln!("{} words, speakers {speakers:?}, {:?}", transcript.words.len(), started.elapsed());
    assert!(speakers.len() >= 2, "diarized");
    assert_eq!(transcript.words[0].speaker.as_deref(), Some("S1"));
}
