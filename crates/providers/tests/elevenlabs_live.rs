//! ElevenLabs live captions against a local fake of the realtime websocket
//! (recorded messages from a real session on the test meeting).

mod fake;

use std::time::Duration;

use fake::{FakeScribe, Step, committed, error, partial, started};
use recap_engine::{AudioFrame, LiveEvent, LiveOptions, LiveStatus, LiveStt, ProviderError, ProviderErrorKind, Source};
use recap_providers::{
    Provider,
    elevenlabs::{Config, ElevenLabs},
};
use tokio::sync::mpsc;

/// Quick to give up and to retry, so tests stay fast.
fn config(base: &str) -> Config {
    Config {
        realtime: base.to_owned(),
        api: "http://127.0.0.1:9".into(),
        backoff: Duration::from_millis(5),
        retries: 3,
        flush: Duration::from_millis(500),
        ..Config::default()
    }
}

fn client(config: Config) -> ElevenLabs {
    ElevenLabs::new("ElevenLabs", "sk_fake", config)
}

/// Frame `i` of 100 ms on the Them side, starting at `offset_ms`, filled
/// with the value `i` so replays can be told apart.
fn frame(i: u64, offset_ms: u64) -> AudioFrame {
    AudioFrame { source: Source::Them, start_ms: offset_ms + i * 100, samples: vec![i as i16; 1600] }
}

struct Session {
    audio: Option<mpsc::UnboundedSender<AudioFrame>>,
    events: mpsc::UnboundedReceiver<LiveEvent>,
    done: tokio::task::JoinHandle<Result<(), ProviderError>>,
}

fn start(stt: &ElevenLabs, options: LiveOptions) -> Session {
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

    /// Events until `n` finals have come.
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

    /// Close the audio and wait for the session to end.
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
    let server = FakeScribe::start(vec![vec![started(), Step::AwaitChunks(2), Step::Close]]).await;
    let stt = client(config(&server.base));
    let mut keyterms: Vec<String> = (0..60).map(|i| format!("term{i}")).collect();
    keyterms[0] = "A very long product name indeed".into();
    let session = start(&stt, LiveOptions { keyterms, language: None });
    session.send((0..2).map(|i| frame(i, 0)));
    let (_, _) = session.finish().await;

    let conn = &server.conns()[0];
    assert!(conn.uri.starts_with("/v1/speech-to-text/realtime?"), "{}", conn.uri);
    for (name, value) in [
        ("model_id", "scribe_v3_live"),
        ("audio_format", "pcm_16000"),
        ("language_code", "en"),
        ("commit_strategy", "turn_prediction"),
        ("no_verbatim", "true"),
        ("include_timestamps", "true"),
    ] {
        assert_eq!(conn.query(name), [value], "{name}");
    }
    let terms = conn.query("keyterms");
    assert_eq!(terms.len(), 50, "at most 50 keyterms");
    assert_eq!(terms[0], "A very long product ", "keyterms are cut to 20 characters");
    assert!(terms.iter().all(|t| t.chars().count() <= 20));
    assert_eq!(conn.key.as_deref(), Some("sk_fake"));

    let first = &conn.sent[0];
    assert_eq!(first["message_type"], "input_audio_chunk");
    assert_eq!(first["sample_rate"], 16000);
    assert_eq!(first["commit"], false);
    assert!(first.get("previous_text").is_none(), "a first session has no context");
    assert_eq!(conn.chunks()[0], vec![0i16; 1600], "100 ms of 16 kHz s16le per chunk");
    assert_eq!(conn.chunks()[1], vec![1i16; 1600]);
}

#[tokio::test]
async fn a_language_option_is_passed_on() {
    let server = FakeScribe::start(vec![vec![started(), Step::AwaitChunks(1), Step::Close]]).await;
    let stt = client(config(&server.base));
    let session = start(&stt, LiveOptions { keyterms: vec![], language: Some("hi".into()) });
    session.send([frame(0, 0)]);
    let _ = session.finish().await;
    assert_eq!(server.conns()[0].query("language_code"), ["hi"]);
    assert!(server.conns()[0].query("keyterms").is_empty());
}

#[tokio::test]
async fn a_recorded_session_gives_partials_then_finals_with_meeting_times() {
    let server = FakeScribe::start(vec![fake::recorded_session()]).await;
    let stt = client(config(&server.base));
    let mut session = start(&stt, LiveOptions::default());
    // The side's timeline is a minute in when this session starts.
    let offset = 60_000;
    session.send((0..260).map(|i| frame(i, offset)));
    let events = session.until_finals(2).await;

    let finals: Vec<&LiveEvent> = events.iter().filter(|e| e.is_final).collect();
    assert!(finals[0].text.starts_with("Okay, let's get started. Thanks for joining, Priya and David."));
    assert!(finals[1].text.starts_with("Thanks, Tom. Quick status from design."));
    // Word times (seconds of session audio) land on the side's timeline.
    assert_eq!((finals[0].start_ms, finals[0].end_ms), (offset + 160, offset + 9_400));
    assert_eq!((finals[1].start_ms, finals[1].end_ms), (offset + 11_040, offset + 23_640));

    let partials: Vec<&LiveEvent> = events.iter().filter(|e| !e.is_final).collect();
    assert!(partials.len() > 20, "partials stream in: {}", partials.len());
    assert!(partials.windows(2).all(|w| w[0].text != w[1].text), "an unchanged partial is not sent again");
    assert!(partials.iter().all(|p| !p.text.is_empty() && p.start_ms <= p.end_ms));
    assert!(partials.iter().all(|p| p.end_ms <= offset + 26_000));
    // The second turn's partials start after the first final.
    let second = events.iter().position(|e| e.is_final).unwrap() + 1;
    assert!(events[second..].iter().filter(|e| !e.is_final).all(|p| p.start_ms >= finals[0].end_ms));

    let (result, rest) = session.finish().await;
    assert_eq!(result, Ok(()));
    assert!(rest.is_empty(), "{rest:?}");
}

#[tokio::test]
async fn closing_the_audio_commits_and_waits_for_the_last_turn() {
    let mut script = vec![started(), Step::AwaitChunks(10), partial("We launch on")];
    script.push(Step::AwaitCommit);
    script.extend(committed("We launch on December 1.", &[("We", 0.2, 0.4), ("1.", 0.9, 1.0)]));
    let server = FakeScribe::start(vec![script]).await;
    let stt = client(config(&server.base));
    let mut session = start(&stt, LiveOptions::default());
    session.send((0..10).map(|i| frame(i, 0)));
    assert_eq!(session.next().await.text, "We launch on");
    let (result, rest) = session.finish().await;
    assert_eq!(result, Ok(()));
    assert_eq!(
        rest,
        [LiveEvent { text: "We launch on December 1.".into(), is_final: true, start_ms: 200, end_ms: 1_000 }]
    );
    let commit = server.conns()[0].sent.last().cloned().unwrap();
    assert_eq!((commit["commit"].clone(), commit["audio_base_64"].clone()), (true.into(), "".into()));
}

#[tokio::test]
async fn an_unanswered_commit_turns_the_partial_final() {
    let server = FakeScribe::start(vec![vec![started(), Step::AwaitChunks(10), partial("We launch on")]]).await;
    let stt = client(config(&server.base));
    let mut session = start(&stt, LiveOptions::default());
    session.send((0..10).map(|i| frame(i, 0)));
    let heard = session.next().await;
    let (result, rest) = session.finish().await;
    assert_eq!(result, Ok(()));
    assert_eq!(rest, [LiveEvent { is_final: true, ..heard }]);
}

#[tokio::test]
async fn silence_keeps_the_socket_alive() {
    let server = FakeScribe::start(vec![vec![started(), Step::AwaitChunks(3), Step::Close]]).await;
    let stt = client(Config { keepalive: Duration::from_millis(30), ..config(&server.base) });
    let session = start(&stt, LiveOptions::default());
    // No audio comes (paused), yet the server hears from us.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let conn = server.conns()[0].clone();
    assert!(conn.chunks().len() >= 3, "keepalive chunks: {}", conn.chunks().len());
    assert!(conn.chunks().iter().all(|c| c.iter().all(|s| *s == 0)), "keepalive is silence");
    drop(session);
}

#[tokio::test]
async fn a_dropped_connection_reconnects_and_replays_audio_since_the_last_final() {
    let mut first = vec![started(), Step::AwaitChunks(20)];
    first.extend(committed("Okay, let's start.", &[("Okay,", 0.1, 0.4), ("start.", 0.7, 1.0)]));
    first.extend([Step::AwaitChunks(40), partial("Thanks for"), Step::Drop]);
    let mut second = vec![started(), Step::AwaitChunks(40)];
    // The replay starts at the first frame after the final (side 1.0 s), so
    // session time 0.2 s is side 1.2 s.
    second.extend(committed("Thanks for joining.", &[("Thanks", 0.2, 0.5), ("joining.", 0.8, 1.5)]));
    let server = FakeScribe::start(vec![first, second]).await;
    let stt = client(config(&server.base));
    let mut session = start(&stt, LiveOptions::default());

    session.send((0..40).map(|i| frame(i, 0)));
    let events = session.until_finals(1).await;
    assert_eq!(events.last().unwrap().text, "Okay, let's start.");
    assert_eq!(session.next().await.text, "Thanks for");
    // The connection drops; audio keeps coming.
    session.send((40..50).map(|i| frame(i, 0)));
    let events = session.until_finals(1).await;
    let last = events.last().unwrap();
    assert_eq!((last.text.as_str(), last.start_ms, last.end_ms), ("Thanks for joining.", 1_200, 2_500));

    let conns = server.conns();
    assert_eq!(conns.len(), 2);
    let replay = &conns[1];
    assert_eq!(replay.sent[0]["previous_text"], "Okay, let's start.", "context for the new session");
    assert!(replay.sent[1..].iter().all(|m| m.get("previous_text").is_none()), "only on the first chunk");
    let firsts: Vec<i16> = replay.chunks().iter().map(|c| c[0]).collect();
    assert_eq!(firsts, (10..50).collect::<Vec<i16>>(), "frames since the last final, then the new ones");
    let (result, _) = session.finish().await;
    assert_eq!(result, Ok(()));
}

#[tokio::test]
async fn a_lost_connection_is_told_as_reconnecting_then_connected() {
    let mut first = vec![started(), Step::AwaitChunks(5)];
    first.push(Step::Drop);
    let mut second = vec![started(), Step::AwaitChunks(5)];
    second.extend(committed("Back again.", &[("Back", 0.0, 0.2), ("again.", 0.2, 0.5)]));
    let server = FakeScribe::start(vec![first, second]).await;
    let stt = client(config(&server.base));
    let (audio, audio_rx) = mpsc::unbounded_channel();
    let (events_tx, mut events) = mpsc::unbounded_channel();
    let (status_tx, mut statuses) = mpsc::unbounded_channel();
    let done = tokio::spawn(stt.run_reporting(Source::Them, LiveOptions::default(), audio_rx, events_tx, status_tx));
    for i in 0..12 {
        audio.send(frame(i, 0)).unwrap();
    }
    let heard = tokio::time::timeout(Duration::from_secs(10), events.recv()).await.unwrap().unwrap();
    assert_eq!(heard.text, "Back again.");
    drop(audio);
    let _ = tokio::time::timeout(Duration::from_secs(10), done).await.unwrap();
    let mut seen = Vec::new();
    while let Ok(s) = statuses.try_recv() {
        seen.push(s);
    }
    assert_eq!(seen, [LiveStatus::Reconnecting, LiveStatus::Connected]);
}

#[tokio::test]
async fn the_session_time_limit_backs_off_and_starts_a_new_session() {
    let first = vec![started(), Step::AwaitChunks(5), error("session_time_limit_exceeded"), Step::Close];
    let mut second = vec![started(), Step::AwaitChunks(5)];
    second.extend(committed("Still here.", &[("Still", 0.0, 0.3), ("here.", 0.3, 0.5)]));
    let server = FakeScribe::start(vec![first, second]).await;
    let stt = client(config(&server.base));
    let mut session = start(&stt, LiveOptions::default());
    session.send((0..10).map(|i| frame(i, 0)));
    let events = session.until_finals(1).await;
    assert_eq!(events.last().unwrap().text, "Still here.");
    assert_eq!(server.conns().len(), 2);
    let (result, _) = session.finish().await;
    assert_eq!(result, Ok(()));
}

#[tokio::test]
async fn transient_server_errors_reconnect() {
    let first = vec![started(), Step::AwaitChunks(1), error("resource_exhausted")];
    let mut second = vec![started(), Step::AwaitChunks(1)];
    second.extend(committed("Back.", &[("Back.", 0.0, 0.1)]));
    let server = FakeScribe::start(vec![first, second]).await;
    let stt = client(config(&server.base));
    let mut session = start(&stt, LiveOptions::default());
    session.send((0..3).map(|i| frame(i, 0)));
    assert_eq!(session.until_finals(1).await.last().unwrap().text, "Back.");
}

#[tokio::test]
async fn an_unreachable_server_gives_up_with_a_network_error() {
    // Nothing listens on the discard port.
    let stt = client(config("ws://127.0.0.1:9"));
    let session = start(&stt, LiveOptions::default());
    session.send((0..3).map(|i| frame(i, 0)));
    let result = tokio::time::timeout(Duration::from_secs(20), session.done).await.unwrap().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Network, "{error:?}");
    assert!(error.falls_back());
}

async fn fails_with(script: Vec<Step>, kind: ProviderErrorKind) {
    let server = FakeScribe::start(vec![script.clone(), script]).await;
    let stt = client(config(&server.base));
    let session = start(&stt, LiveOptions::default());
    session.send((0..3).map(|i| frame(i, 0)));
    let result = tokio::time::timeout(Duration::from_secs(10), session.done).await.unwrap().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.kind, kind, "{error:?}");
    assert!(error.falls_back(), "the job moves to its backup");
    assert_eq!(server.conns().len(), 1, "no retry on {kind:?}");
}

#[tokio::test]
async fn auth_error_is_a_typed_auth_error() {
    fails_with(vec![error("auth_error"), Step::Close], ProviderErrorKind::Auth).await;
}

#[tokio::test]
async fn quota_exceeded_is_a_typed_quota_error() {
    fails_with(vec![started(), Step::AwaitChunks(1), error("quota_exceeded"), Step::Close], ProviderErrorKind::Quota)
        .await;
}

#[tokio::test]
async fn a_refused_handshake_is_an_auth_error() {
    fails_with(vec![Step::Reject(401)], ProviderErrorKind::Auth).await;
}

#[tokio::test]
async fn test_opens_a_session() {
    let server = FakeScribe::start(vec![vec![started()]]).await;
    let provider = client(config(&server.base));
    assert_eq!(provider.test().await, Ok(()));
    assert_eq!(server.conns()[0].key.as_deref(), Some("sk_fake"));

    let server = FakeScribe::start(vec![vec![error("auth_error"), Step::Close]]).await;
    let provider = client(config(&server.base));
    assert_eq!(provider.test().await.unwrap_err().kind, ProviderErrorKind::Auth);
}

#[tokio::test]
async fn it_offers_live_and_batch() {
    let provider = client(Config::default());
    assert!(provider.live().is_some());
    assert!(provider.batch().is_some());
    let caps = provider.live().unwrap().capabilities();
    assert!(caps.live && caps.batch && caps.diarize && caps.keyterms && caps.word_timestamps);
    assert_eq!(provider.live().unwrap().name(), "ElevenLabs");
}
