//! The Live captions and Transcript jobs as built by default (ElevenLabs,
//! backed by Deepgram), each adapter against its local fake: ElevenLabs
//! runs out of quota and Deepgram takes the job over, the user told which.

mod fake;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use fake::{FakeApi, FakeScribe, Step, deepgram::FakeListen, error, started};
use recap_engine::{
    AudioFrame, BatchRequest, BatchStt, Fallback, FallbackNotice, LiveEvent, LiveOptions, LiveStt, ProviderErrorKind,
    Source,
};
use recap_providers::{deepgram, elevenlabs};
use tokio::sync::mpsc;

fn elevenlabs(realtime: &str, api: &str) -> Arc<elevenlabs::ElevenLabs> {
    let config = elevenlabs::Config {
        realtime: realtime.into(),
        api: api.into(),
        backoff: Duration::from_millis(5),
        flush: Duration::from_millis(300),
        ..elevenlabs::Config::default()
    };
    Arc::new(elevenlabs::ElevenLabs::new("ElevenLabs", "sk_fake", config))
}

fn deepgram(live: &str, api: &str) -> Arc<deepgram::Deepgram> {
    let config = deepgram::Config {
        live: live.into(),
        api: api.into(),
        backoff: Duration::from_millis(5),
        flush: Duration::from_millis(300),
        ..deepgram::Config::default()
    };
    Arc::new(deepgram::Deepgram::new("Deepgram", "dg_fake", config))
}

fn notices() -> (Arc<Mutex<Vec<FallbackNotice>>>, impl Fn(FallbackNotice) + Send + Sync + 'static) {
    let seen: Arc<Mutex<Vec<FallbackNotice>>> = Arc::default();
    let sink = seen.clone();
    (seen, move |n| sink.lock().unwrap().push(n))
}

#[tokio::test]
async fn out_of_elevenlabs_quota_live_captions_go_on_with_deepgram() {
    // ElevenLabs hears 5 frames, then says its quota is gone.
    let scribe = FakeScribe::start(vec![vec![started(), Step::AwaitChunks(5), error("quota_exceeded"), Step::Close]]).await;
    // Deepgram gets the test meeting's first two turns.
    let listen = FakeListen::start(vec![fake::deepgram::recorded_session()]).await;
    let (seen, on_switch) = notices();
    let job = Fallback::new(
        elevenlabs(&scribe.base, "http://127.0.0.1:9") as Arc<dyn LiveStt>,
        Some(deepgram(&listen.base, "http://127.0.0.1:9") as Arc<dyn LiveStt>),
    )
    .on_switch(on_switch);
    let switch = job.switch();
    assert_eq!((switch.active(), switch.backup()), ("ElevenLabs".into(), Some("Deepgram".into())));

    let (audio, audio_rx) = mpsc::unbounded_channel();
    let (events_tx, mut events) = mpsc::unbounded_channel::<LiveEvent>();
    let session = tokio::spawn(job.run(Source::Them, LiveOptions::default(), audio_rx, events_tx));
    for i in 0..260u64 {
        audio.send(AudioFrame { source: Source::Them, start_ms: i * 100, samples: vec![i as i16; 1600] }).unwrap();
    }
    let mut finals = Vec::new();
    while finals.len() < 11 {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv()).await.expect("captions go on").unwrap();
        if event.is_final {
            finals.push(event);
        }
    }
    assert_eq!(finals[0].text, "Okay,");
    assert_eq!(finals[10].text, "I still need to fix the dark mode colors on the payment page.");
    // The audio ElevenLabs never captioned is replayed to Deepgram from the
    // start, so the times are the meeting's.
    assert_eq!((finals[0].start_ms, finals[0].end_ms), (160, 440));
    assert_eq!(listen.conns()[0].audio[0], vec![0i16; 1600], "replayed from the first unheard frame");

    // The user sees who captions now.
    let told = seen.lock().unwrap().clone();
    assert_eq!(told.len(), 1);
    assert_eq!((told[0].from.as_str(), told[0].to.as_str(), told[0].error.kind), ("ElevenLabs", "Deepgram", ProviderErrorKind::Quota));
    assert_eq!((switch.active(), switch.backup()), ("Deepgram".into(), None));

    drop(audio);
    let result = tokio::time::timeout(Duration::from_secs(10), session).await.unwrap().unwrap();
    assert_eq!(result, Ok(()));
    assert_eq!(scribe.conns().len(), 1, "no retry on quota");
}

#[tokio::test]
async fn out_of_elevenlabs_quota_the_transcript_comes_from_deepgram() {
    let quota = r#"{"detail":{"status":"quota_exceeded","message":"This request exceeds your quota."}}"#;
    let elevenlabs_api = FakeApi::start(vec![(401, quota.into())]).await;
    let deepgram_api = FakeApi::start(vec![(200, include_str!("fixtures/deepgram_batch.json").into())]).await;
    let (seen, on_switch) = notices();
    let job = Fallback::new(
        elevenlabs("ws://127.0.0.1:9", &elevenlabs_api.base) as Arc<dyn BatchStt>,
        Some(deepgram("ws://127.0.0.1:9", &deepgram_api.base) as Arc<dyn BatchStt>),
    )
    .on_switch(on_switch);
    let transcript =
        job.transcribe(BatchRequest { samples: vec![0; 16_000], diarize: true, keyterms: vec![] }).await.unwrap();
    assert_eq!(transcript.words.len(), 102);
    assert_eq!(transcript.words[0].speaker.as_deref(), Some("S1"));
    let told = seen.lock().unwrap().clone();
    assert_eq!((told[0].from.as_str(), told[0].to.as_str(), told[0].error.kind), ("ElevenLabs", "Deepgram", ProviderErrorKind::Quota));
    assert_eq!(deepgram_api.requests().len(), 1);
}
