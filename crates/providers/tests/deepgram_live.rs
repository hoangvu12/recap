//! Deepgram live captions (Nova-3 over `/v1/listen`) against a local fake.
//! There is no Deepgram key, so the fixture is not a recording: it is
//! Deepgram's documented `Results` / `UtteranceEnd` messages over the test
//! meeting's first two turns (tools/gen_deepgram_fixtures.py).

mod fake;

use std::time::Duration;

use fake::deepgram::{FakeListen, Step, interim, metadata, results, utterance_end};
use recap_engine::{AudioFrame, LiveEvent, LiveOptions, LiveStt, ProviderError, ProviderErrorKind, Source};
use recap_providers::{
    Provider,
    deepgram::{Config, Deepgram},
};
use tokio::sync::mpsc;

fn config(base: &str) -> Config {
    Config {
        live: base.to_owned(),
        api: "http://127.0.0.1:9".into(),
        backoff: Duration::from_millis(5),
        retries: 3,
        flush: Duration::from_millis(500),
        ..Config::default()
    }
}

fn client(config: Config) -> Deepgram {
    Deepgram::new("Deepgram", "dg_fake", config)
}

/// Frame `i` of 100 ms on the Them side from `offset_ms`, filled with `i`.
fn frame(i: u64, offset_ms: u64) -> AudioFrame {
    AudioFrame { source: Source::Them, start_ms: offset_ms + i * 100, samples: vec![i as i16; 1600] }
}

struct Session {
    audio: Option<mpsc::UnboundedSender<AudioFrame>>,
    events: mpsc::UnboundedReceiver<LiveEvent>,
    done: tokio::task::JoinHandle<Result<(), ProviderError>>,
}

fn start(stt: &Deepgram, options: LiveOptions) -> Session {
    let (audio, audio_rx) = mpsc::unbounded_channel();
    let (events_tx, events) = mpsc::unbounded_channel();
    let done = tokio::spawn(stt.run(Source::Them, options, audio_rx, events_tx));
    Session { audio: Some(audio), events, done }
}

impl Session {
    fn send(&self, frames: impl IntoIterator<Item = AudioFrame>) {
        for frame in frames {
            self.audio.as_ref().unwrap().send(frame).unwrap();
        }
    }

    async fn until_finals(&mut self, n: usize) -> Vec<LiveEvent> {
        let mut out = Vec::new();
        while out.iter().filter(|e: &&LiveEvent| e.is_final).count() < n {
            let event = tokio::time::timeout(Duration::from_secs(10), self.events.recv())
                .await
                .expect("an event in time")
                .expect("the session is still sending");
            out.push(event);
        }
        out
    }

    async fn next(&mut self) -> LiveEvent {
        tokio::time::timeout(Duration::from_secs(10), self.events.recv()).await.unwrap().unwrap()
    }

    async fn finish(mut self) -> (Result<(), ProviderError>, Vec<LiveEvent>) {
        self.audio = None;
        let result = tokio::time::timeout(Duration::from_secs(10), self.done).await.expect("ends in time").unwrap();
        let mut rest = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            rest.push(event);
        }
        (result, rest)
    }
}

#[tokio::test]
async fn connects_with_the_tested_parameters() {
    let server =
        FakeListen::start(vec![vec![Step::AwaitAudio(2), Step::AwaitText("CloseStream"), Step::Close(1000, "")]]).await;
    let stt = client(config(&server.base));
    let mut keyterms: Vec<String> = (0..60).map(|i| format!("term{i}")).collect();
    keyterms[0] = "A very long product name indeed".into();
    let session = start(&stt, LiveOptions { keyterms, language: None });
    session.send((0..2).map(|i| frame(i, 0)));
    let (result, _) = session.finish().await;
    assert_eq!(result, Ok(()));

    let conn = &server.conns()[0];
    assert!(conn.uri.starts_with("/v1/listen?"), "{}", conn.uri);
    // The Python app's live settings (live_captions.py).
    for (name, value) in [
        ("model", "nova-3"),
        ("language", "en"),
        ("encoding", "linear16"),
        ("sample_rate", "16000"),
        ("channels", "1"),
        ("interim_results", "true"),
        ("smart_format", "true"),
        ("endpointing", "300"),
        ("utterance_end_ms", "1000"),
    ] {
        assert_eq!(conn.query(name), [value], "{name}");
    }
    let terms = conn.query("keyterm");
    assert_eq!(terms.len(), 50);
    assert_eq!(terms[0], "A very long product ");
    assert_eq!(conn.authorization.as_deref(), Some("Token dg_fake"));
    // Audio goes as binary 16-bit little-endian frames, 100 ms each.
    assert_eq!(conn.audio, [vec![0i16; 1600], vec![1i16; 1600]]);
    assert_eq!(conn.sent("CloseStream"), 1, "closing the audio closes the stream");
}

#[tokio::test]
async fn a_language_option_is_passed_on() {
    let server = FakeListen::start(vec![vec![Step::AwaitText("CloseStream"), Step::Close(1000, "")]]).await;
    let session =
        start(&client(config(&server.base)), LiveOptions { keyterms: vec![], language: Some("en-IN".into()) });
    session.send([frame(0, 0)]);
    let _ = session.finish().await;
    assert_eq!(server.conns()[0].query("language"), ["en-IN"]);
    assert!(server.conns()[0].query("keyterm").is_empty());
}

// Language Auto (ADR 0022): nova-3's multilingual code-switching.
#[tokio::test]
async fn language_auto_is_multi() {
    let server = FakeListen::start(vec![vec![Step::AwaitText("CloseStream"), Step::Close(1000, "")]]).await;
    let session = start(&client(config(&server.base)), LiveOptions { keyterms: vec![], language: Some("auto".into()) });
    session.send([frame(0, 0)]);
    let _ = session.finish().await;
    assert_eq!(server.conns()[0].query("language"), ["multi"]);
}

#[tokio::test]
async fn the_test_meeting_gives_partials_then_finals_with_meeting_times() {
    let server = FakeListen::start(vec![fake::deepgram::recorded_session()]).await;
    let stt = client(config(&server.base));
    let mut session = start(&stt, LiveOptions::default());
    let offset = 60_000;
    session.send((0..260).map(|i| frame(i, offset)));
    let events = session.until_finals(11).await;

    let finals: Vec<&LiveEvent> = events.iter().filter(|e| e.is_final).collect();
    let texts: Vec<&str> = finals.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(
        texts[..5],
        [
            "Okay,",
            "let's get started.",
            "Thanks for joining,",
            "Priya and David.",
            "Today, I want to settle the launch date for the mobile app and agree on who owns the remaining work."
        ]
    );
    assert_eq!(texts[5..7], ["Thanks,", "Tom."]);
    assert_eq!(texts[10], "I still need to fix the dark mode colors on the payment page.");
    // Word times (seconds of the audio sent) land on the side's timeline.
    assert_eq!((finals[0].start_ms, finals[0].end_ms), (offset + 160, offset + 440));
    assert_eq!((finals[4].start_ms, finals[4].end_ms), (offset + 5_080, offset + 9_400));
    assert_eq!((finals[10].start_ms, finals[10].end_ms), (offset + 20_460, offset + 23_640));

    let partials: Vec<&LiveEvent> = events.iter().filter(|e| !e.is_final).collect();
    assert!(partials.len() >= 10, "interim results stream in: {}", partials.len());
    assert!(partials.windows(2).all(|w| w[0].text != w[1].text), "an unchanged partial is not sent again");
    assert!(partials.iter().all(|p| !p.text.is_empty() && p.start_ms <= p.end_ms));
    // Partials of the long sentence grow until its final.
    assert!(partials.iter().any(|p| p.text.starts_with("Today, I want") && p.text.len() < finals[4].text.len()));

    let (result, rest) = session.finish().await;
    assert_eq!(result, Ok(()));
    assert!(rest.is_empty(), "{rest:?}");
}

#[tokio::test]
async fn finals_of_one_utterance_join_until_speech_final_or_utterance_end() {
    let script = vec![
        Step::AwaitAudio(16),
        interim(&[("We", 0.2, 0.4)]),
        results(&[("We", 0.2, 0.4), ("launch", 0.4, 0.7)], true, false),
        interim(&[("on", 0.8, 0.9)]),
        results(&[("on", 0.8, 0.9), ("December", 0.9, 1.3), ("1.", 1.3, 1.5)], true, true),
        Step::AwaitAudio(30),
        results(&[("Agreed.", 2.0, 2.4)], true, false),
        utterance_end(2.4),
        Step::AwaitText("CloseStream"),
        metadata(),
        Step::Close(1000, ""),
    ];
    let server = FakeListen::start(vec![script]).await;
    let mut session = start(&client(config(&server.base)), LiveOptions::default());
    // 1.6 s of audio: the words' times stay inside what was sent.
    session.send((0..16).map(|i| frame(i, 0)));
    let events = session.until_finals(1).await;
    let texts: Vec<(&str, bool)> = events.iter().map(|e| (e.text.as_str(), e.is_final)).collect();
    assert_eq!(
        texts,
        [("We", false), ("We launch", false), ("We launch on", false), ("We launch on December 1.", true)]
    );
    assert_eq!((events[3].start_ms, events[3].end_ms), (200, 1_500));
    session.send((16..30).map(|i| frame(i, 0)));
    let events = session.until_finals(1).await;
    assert_eq!(
        events.last().unwrap(),
        &LiveEvent { text: "Agreed.".into(), is_final: true, start_ms: 2_000, end_ms: 2_400 }
    );
    let (result, rest) = session.finish().await;
    assert_eq!(result, Ok(()));
    assert!(rest.is_empty());
}

#[tokio::test]
async fn closing_the_audio_flushes_the_last_words() {
    let script = vec![
        Step::AwaitAudio(10),
        interim(&[("We", 0.2, 0.4), ("launch", 0.4, 0.7)]),
        Step::AwaitText("CloseStream"),
        results(&[("We", 0.2, 0.4), ("launch", 0.4, 0.7), ("Monday.", 0.7, 1.0)], true, false),
        metadata(),
        Step::Close(1000, ""),
    ];
    let server = FakeListen::start(vec![script]).await;
    let mut session = start(&client(config(&server.base)), LiveOptions::default());
    session.send((0..10).map(|i| frame(i, 0)));
    assert_eq!(session.next().await.text, "We launch");
    let (result, rest) = session.finish().await;
    assert_eq!(result, Ok(()));
    assert_eq!(rest, [LiveEvent { text: "We launch Monday.".into(), is_final: true, start_ms: 200, end_ms: 1_000 }]);
}

#[tokio::test]
async fn an_unanswered_close_turns_the_partial_final() {
    let server =
        FakeListen::start(vec![vec![Step::AwaitAudio(10), interim(&[("We", 0.2, 0.4), ("launch", 0.4, 0.7)])]]).await;
    let mut session = start(&client(config(&server.base)), LiveOptions::default());
    session.send((0..10).map(|i| frame(i, 0)));
    let heard = session.next().await;
    let (result, rest) = session.finish().await;
    assert_eq!(result, Ok(()));
    assert_eq!(rest, [LiveEvent { is_final: true, ..heard }]);
}

#[tokio::test]
async fn silence_keeps_the_socket_alive() {
    let server = FakeListen::start(vec![vec![]]).await;
    let stt = client(Config { keepalive: Duration::from_millis(30), ..config(&server.base) });
    let session = start(&stt, LiveOptions::default());
    // Paused: no audio, yet Deepgram hears from us (it closes after 10 s).
    tokio::time::sleep(Duration::from_millis(400)).await;
    let conn = server.conns()[0].clone();
    assert!(conn.sent("KeepAlive") >= 3, "keepalives: {}", conn.sent("KeepAlive"));
    assert!(conn.audio.is_empty(), "a keepalive is a text message, not audio");
    drop(session);
}

#[tokio::test]
async fn a_dropped_connection_reconnects_and_replays_audio_since_the_last_final() {
    let first = vec![
        Step::AwaitAudio(20),
        results(&[("Okay,", 0.1, 0.4), ("start.", 0.7, 1.0)], true, true),
        Step::AwaitAudio(40),
        interim(&[("Thanks", 1.2, 1.5), ("for", 1.5, 1.7)]),
        Step::Drop,
    ];
    // The replay starts at the first frame after the final (side 1.0 s), so
    // 0.2 s of the new connection's audio is side 1.2 s.
    let second = vec![
        Step::AwaitAudio(40),
        results(&[("Thanks", 0.2, 0.5), ("for", 0.5, 0.7), ("joining.", 0.8, 1.5)], true, true),
        Step::AwaitText("CloseStream"),
        Step::Close(1000, ""),
    ];
    let server = FakeListen::start(vec![first, second]).await;
    let mut session = start(&client(config(&server.base)), LiveOptions::default());
    session.send((0..40).map(|i| frame(i, 0)));
    assert_eq!(session.until_finals(1).await.last().unwrap().text, "Okay, start.");
    assert_eq!(session.next().await.text, "Thanks for");
    session.send((40..50).map(|i| frame(i, 0)));
    let events = session.until_finals(1).await;
    let last = events.last().unwrap();
    assert_eq!((last.text.as_str(), last.start_ms, last.end_ms), ("Thanks for joining.", 1_200, 2_500));

    let conns = server.conns();
    assert_eq!(conns.len(), 2);
    let firsts: Vec<i16> = conns[1].audio.iter().map(|c| c[0]).collect();
    assert_eq!(firsts, (10..50).collect::<Vec<i16>>(), "frames since the last final, then the new ones");
    let (result, _) = session.finish().await;
    assert_eq!(result, Ok(()));
}

#[tokio::test]
async fn a_server_side_close_reconnects() {
    // NET-0001: Deepgram heard nothing for 10 s; NET-0000: its own error.
    for reason in ["NET-0001", "NET-0000"] {
        let first = vec![Step::AwaitAudio(1), Step::Close(1011, reason)];
        let second = vec![Step::AwaitAudio(1), results(&[("Back.", 0.0, 0.1)], true, true)];
        let server = FakeListen::start(vec![first, second]).await;
        let mut session = start(&client(config(&server.base)), LiveOptions::default());
        session.send((0..3).map(|i| frame(i, 0)));
        assert_eq!(session.until_finals(1).await.last().unwrap().text, "Back.", "{reason}");
        assert_eq!(server.conns().len(), 2);
    }
}

#[tokio::test]
async fn an_unreachable_server_gives_up_with_a_network_error() {
    let session = start(&client(config("ws://127.0.0.1:9")), LiveOptions::default());
    session.send((0..3).map(|i| frame(i, 0)));
    let result = tokio::time::timeout(Duration::from_secs(20), session.done).await.unwrap().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Network, "{error:?}");
    assert!(error.falls_back());
}

async fn fails_with(script: Vec<Step>, kind: ProviderErrorKind) -> ProviderError {
    let server = FakeListen::start(vec![script.clone(), script]).await;
    let session = start(&client(config(&server.base)), LiveOptions::default());
    session.send((0..3).map(|i| frame(i, 0)));
    let result = tokio::time::timeout(Duration::from_secs(10), session.done).await.unwrap().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.kind, kind, "{error:?}");
    assert_eq!(server.conns().len(), 1, "no retry on {kind:?}");
    error
}

#[tokio::test]
async fn a_refused_key_is_an_auth_error() {
    let body = r#"{"err_code":"INVALID_AUTH","err_msg":"Invalid credentials.","request_id":"x"}"#;
    let error = fails_with(vec![Step::Reject(401, body)], ProviderErrorKind::Auth).await;
    assert!(error.message.contains("Invalid credentials."), "{}", error.message);
    assert!(error.falls_back());
}

#[tokio::test]
async fn no_credit_is_a_quota_error() {
    let body = r#"{"err_code":"ASR_PAYMENT_REQUIRED","err_msg":"Project does not have enough credits for an ASR request and does not have an overage agreement.","request_id":"x"}"#;
    let error = fails_with(vec![Step::Reject(402, body)], ProviderErrorKind::Quota).await;
    assert!(error.message.starts_with("Deepgram quota"), "{}", error.message);
    assert!(error.falls_back());
}

#[tokio::test]
async fn a_bad_message_close_is_a_protocol_error() {
    fails_with(vec![Step::AwaitAudio(1), Step::Close(1008, "DATA-0000")], ProviderErrorKind::Protocol).await;
}

#[tokio::test]
async fn test_opens_a_stream() {
    let server = FakeListen::start(vec![vec![Step::AwaitText("CloseStream"), Step::Close(1000, "")]]).await;
    assert_eq!(client(config(&server.base)).test().await, Ok(()));
    assert_eq!(server.conns()[0].authorization.as_deref(), Some("Token dg_fake"));
    assert!(server.conns()[0].audio.is_empty(), "the Test sends no audio");

    let server = FakeListen::start(vec![vec![Step::Reject(
        401,
        r#"{"err_code":"INVALID_AUTH","err_msg":"Invalid credentials."}"#,
    )]])
    .await;
    assert_eq!(client(config(&server.base)).test().await.unwrap_err().kind, ProviderErrorKind::Auth);
}

#[tokio::test]
async fn it_offers_live_and_batch() {
    let provider = client(Config::default());
    assert!(provider.live().is_some());
    assert!(provider.batch().is_some());
    let caps = provider.live().unwrap().capabilities();
    assert!(caps.live && caps.batch && caps.diarize && caps.keyterms && caps.word_timestamps);
    assert_eq!(provider.live().unwrap().name(), "Deepgram");
}
