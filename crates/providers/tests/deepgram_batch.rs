//! Deepgram pre-recorded (after Stop) against a local fake of the HTTP API.
//! The fixture is Deepgram's documented response shape over the test
//! meeting's words (tools/gen_deepgram_fixtures.py), its speakers numbered
//! out of order as Deepgram may.

mod fake;

use std::time::Duration;

use fake::FakeApi;
use recap_engine::{BatchRequest, BatchStt, ProviderErrorKind};
use recap_providers::deepgram::{Config, Deepgram};

const RESPONSE: &str = include_str!("fixtures/deepgram_batch.json");

fn client(api: &str) -> Deepgram {
    Deepgram::new(
        "Deepgram",
        "dg_fake",
        Config { api: api.to_owned(), live: "ws://127.0.0.1:9".into(), ..Config::default() },
    )
}

fn request(diarize: bool) -> BatchRequest {
    BatchRequest { samples: vec![7; 16_000], diarize, keyterms: vec!["Priya".into(), "Recap".into()], language: None }
}

#[tokio::test]
async fn uploads_the_side_with_the_tested_parameters() {
    let api = FakeApi::start(vec![(200, RESPONSE.into())]).await;
    client(&api.base).transcribe(request(true)).await.unwrap();

    let sent = &api.requests()[0];
    let line = sent.head.lines().next().unwrap();
    assert!(line.starts_with("POST /v1/listen?"), "{line}");
    let target = line.split(' ').nth(1).unwrap();
    let url = reqwest::Url::parse(&format!("http://x{target}")).unwrap();
    let query =
        |name: &str| url.query_pairs().filter(|(k, _)| k == name).map(|(_, v)| v.into_owned()).collect::<Vec<_>>();
    // transcribe_file.py's settings.
    for (name, value) in
        [("model", "nova-3"), ("language", "en"), ("smart_format", "true"), ("punctuate", "true"), ("diarize", "true")]
    {
        assert_eq!(query(name), [value], "{name}");
    }
    assert_eq!(query("keyterm"), ["Priya", "Recap"]);
    assert_eq!(sent.header("authorization").as_deref(), Some("Token dg_fake"));
    assert_eq!(sent.header("content-type").as_deref(), Some("audio/wav"));

    // The side goes up as the body: a 16 kHz mono 16-bit WAV.
    let file = &sent.body;
    assert_eq!(&file[..4], b"RIFF");
    assert_eq!(&file[8..12], b"WAVE");
    assert_eq!(u16::from_le_bytes([file[22], file[23]]), 1, "mono");
    assert_eq!(u32::from_le_bytes([file[24], file[25], file[26], file[27]]), 16_000);
    assert_eq!(file.len(), 44 + 32_000);
}

#[tokio::test]
async fn words_come_back_punctuated_in_ms_with_diarized_speakers() {
    let api = FakeApi::start(vec![(200, RESPONSE.into())]).await;
    let words = client(&api.base).transcribe(request(true)).await.unwrap().words;

    assert_eq!(words.len(), 102);
    assert_eq!((words[0].text.as_str(), words[0].start_ms, words[0].end_ms), ("Okay,", 160, 440));
    let text: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    assert!(text.join(" ").contains("Thanks, Tom. Quick status from design."));
    // Deepgram numbered Tom 1, Priya 0, David 2; speakers are S1.. in order
    // of first appearance.
    assert_eq!(words[0].speaker.as_deref(), Some("S1"));
    let at = |w: &str| words.iter().position(|x| x.text == w).unwrap();
    assert_eq!(words[at("David.")].speaker.as_deref(), Some("S1"), "Tom: thanks for joining, Priya and David");
    assert_eq!(words[at("Tom.")].speaker.as_deref(), Some("S2"), "Priya: thanks, Tom");
    assert_eq!(words[at("backend")].speaker.as_deref(), Some("S3"), "David: from the backend side");
    assert!(words.windows(2).all(|w| w[0].start_ms <= w[1].start_ms));
}

#[tokio::test]
async fn without_diarize_there_are_no_speakers() {
    let api = FakeApi::start(vec![(200, RESPONSE.into())]).await;
    let words = client(&api.base).transcribe(request(false)).await.unwrap().words;
    assert!(words.iter().all(|w| w.speaker.is_none()));
    assert!(api.requests()[0].head.lines().next().unwrap().contains("diarize=false"));
}

#[tokio::test]
async fn no_speech_is_no_words() {
    let empty = r#"{"metadata":{"request_id":"x","duration":1.0,"channels":1},"results":{"channels":[{"alternatives":[{"transcript":"","confidence":0.0,"words":[]}]}]}}"#;
    let api = FakeApi::start(vec![(200, empty.into())]).await;
    assert!(client(&api.base).transcribe(request(true)).await.unwrap().words.is_empty());
}

async fn fails_with(status: u16, body: &str, kind: ProviderErrorKind) {
    let api = FakeApi::start(vec![(status, body.into())]).await;
    let error = client(&api.base).transcribe(request(true)).await.unwrap_err();
    assert_eq!(error.kind, kind, "{status} {body}: {error:?}");
}

#[tokio::test]
async fn http_errors_are_typed() {
    fails_with(
        401,
        r#"{"err_code":"INVALID_AUTH","err_msg":"Invalid credentials.","request_id":"x"}"#,
        ProviderErrorKind::Auth,
    )
    .await;
    fails_with(
        403,
        r#"{"err_code":"INSUFFICIENT_PERMISSIONS","err_msg":"no","request_id":"x"}"#,
        ProviderErrorKind::Auth,
    )
    .await;
    fails_with(
        402,
        r#"{"err_code":"ASR_PAYMENT_REQUIRED","err_msg":"Project does not have enough credits for an ASR request and does not have an overage agreement.","request_id":"x"}"#,
        ProviderErrorKind::Quota,
    )
    .await;
    fails_with(429, r#"{"err_code":"TOO_MANY_REQUESTS","err_msg":"slow down"}"#, ProviderErrorKind::Quota).await;
    fails_with(503, "{}", ProviderErrorKind::Network).await;
    fails_with(
        400,
        r#"{"err_code":"Bad Request","err_msg":"corrupt or unsupported data"}"#,
        ProviderErrorKind::Protocol,
    )
    .await;
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
