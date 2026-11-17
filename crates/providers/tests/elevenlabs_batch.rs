//! ElevenLabs batch (after Stop) against a local fake of the HTTP API that
//! replays a real `scribe_v3` response on the test meeting.

mod fake;

use std::time::Duration;

use fake::FakeApi;
use recap_engine::{BatchRequest, BatchStt, ProviderErrorKind};
use recap_providers::elevenlabs::{Config, ElevenLabs};

const RECORDED: &str = include_str!("fixtures/elevenlabs_batch.json");

fn client(api: &str) -> ElevenLabs {
    let config = Config { api: api.to_owned(), realtime: "ws://127.0.0.1:9".into(), ..Config::default() };
    ElevenLabs::new("ElevenLabs", "sk_fake", config)
}

fn request(diarize: bool) -> BatchRequest {
    BatchRequest { samples: vec![7; 16_000], diarize, keyterms: vec!["Priya".into(), "Recap".into()], language: None }
}

#[tokio::test]
async fn uploads_the_side_with_the_tested_parameters() {
    let api = FakeApi::start(vec![(200, RECORDED.into())]).await;
    client(&api.base).transcribe(request(true)).await.unwrap();

    let sent = &api.requests()[0];
    assert!(sent.head.starts_with("POST /v1/speech-to-text "), "{}", sent.head);
    assert_eq!(sent.header("xi-api-key").as_deref(), Some("sk_fake"));
    let fields = sent.fields();
    let field = |name: &str| fields.iter().filter(|(k, _)| k == name).map(|(_, v)| v.as_str()).collect::<Vec<_>>();
    assert_eq!(field("model_id"), ["scribe_v3"]);
    assert_eq!(field("language_code"), ["en"]);
    assert_eq!(field("diarize"), ["true"]);
    assert_eq!(field("no_verbatim"), ["true"]);
    assert_eq!(field("timestamps_granularity"), ["word"]);
    assert_eq!(field("keyterms"), ["Priya", "Recap"]);

    // The side goes up as a 16 kHz mono 16-bit WAV.
    let file = sent.file().expect("a file part");
    assert_eq!(&file[..4], b"RIFF");
    assert_eq!(&file[8..12], b"WAVE");
    assert_eq!(u16::from_le_bytes([file[22], file[23]]), 1, "mono");
    assert_eq!(u32::from_le_bytes([file[24], file[25], file[26], file[27]]), 16_000);
    assert_eq!(file.len(), 44 + 32_000);
    assert_eq!(&file[44..46], &7i16.to_le_bytes());
}

// Language Auto (ADR 0022): Scribe detects the language.
#[tokio::test]
async fn language_auto_leaves_the_language_to_scribe() {
    let api = FakeApi::start(vec![(200, RECORDED.into()), (200, RECORDED.into())]).await;
    let stt = client(&api.base);
    stt.transcribe(BatchRequest { language: Some("auto".into()), ..request(false) }).await.unwrap();
    stt.transcribe(BatchRequest { language: Some("vi".into()), ..request(false) }).await.unwrap();
    let languages: Vec<Vec<String>> = api
        .requests()
        .iter()
        .map(|r| r.fields().into_iter().filter(|(k, _)| k == "language_code").map(|(_, v)| v).collect())
        .collect();
    assert_eq!(languages, [vec![], vec!["vi".to_owned()]]);
}

#[tokio::test]
async fn words_come_back_in_ms_with_diarized_speakers() {
    let api = FakeApi::start(vec![(200, RECORDED.into())]).await;
    let transcript = client(&api.base).transcribe(request(true)).await.unwrap();
    let words = &transcript.words;

    // The fixture has 203 entries: 102 words and the spacing between them.
    assert_eq!(words.len(), 102, "words only, no spacing");
    assert_eq!((words[0].text.as_str(), words[0].start_ms, words[0].end_ms), ("Okay,", 160, 440));
    assert_eq!(words[0].speaker.as_deref(), Some("S1"));
    let text: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    assert!(text.join(" ").contains("Thanks, Tom."));
    // Speakers are S1.. in order of first appearance.
    let mut speakers: Vec<&str> = Vec::new();
    for w in words {
        let s = w.speaker.as_deref().unwrap();
        if !speakers.contains(&s) {
            speakers.push(s);
        }
    }
    assert_eq!(speakers, ["S1", "S2", "S3"]);
    assert!(words.windows(2).all(|w| w[0].start_ms <= w[1].start_ms));
}

#[tokio::test]
async fn without_diarize_there_are_no_speakers() {
    let api = FakeApi::start(vec![(200, RECORDED.into())]).await;
    let transcript = client(&api.base).transcribe(request(false)).await.unwrap();
    assert!(transcript.words.iter().all(|w| w.speaker.is_none()));
    let fields = api.requests()[0].fields();
    assert!(fields.iter().any(|(k, v)| k == "diarize" && v == "false"));
}

async fn fails_with(status: u16, body: &str, kind: ProviderErrorKind) {
    let api = FakeApi::start(vec![(status, body.into())]).await;
    let error = client(&api.base).transcribe(request(true)).await.unwrap_err();
    assert_eq!(error.kind, kind, "{status} {body}: {error:?}");
}

#[tokio::test]
async fn http_errors_are_typed() {
    let unauthorized = r#"{"detail":{"status":"invalid_api_key","message":"Invalid API key"}}"#;
    fails_with(401, unauthorized, ProviderErrorKind::Auth).await;
    let quota = r#"{"detail":{"status":"quota_exceeded","message":"This request exceeds your quota."}}"#;
    fails_with(401, quota, ProviderErrorKind::Quota).await;
    fails_with(402, r#"{"detail":"payment required"}"#, ProviderErrorKind::Quota).await;
    fails_with(429, r#"{"detail":{"status":"too_many_concurrent_requests"}}"#, ProviderErrorKind::Quota).await;
    fails_with(503, "{}", ProviderErrorKind::Network).await;
    fails_with(400, r#"{"detail":{"status":"invalid_file","message":"bad file"}}"#, ProviderErrorKind::Protocol).await;
    fails_with(200, "not json", ProviderErrorKind::Protocol).await;
}

#[tokio::test]
async fn an_unreachable_api_is_a_network_error() {
    let error = tokio::time::timeout(Duration::from_secs(20), client("http://127.0.0.1:9").transcribe(request(true)))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Network);
}
