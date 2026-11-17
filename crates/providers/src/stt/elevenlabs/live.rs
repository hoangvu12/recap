//! Live captions over the realtime websocket (`scribe_v3_live`).
//!
//! Protocol (reference/elevenlabs_run.py, probed against the real API):
//! - `GET /v1/speech-to-text/realtime?model_id=scribe_v3_live&audio_format=pcm_16000&language_code=en
//!   &commit_strategy=turn_prediction&no_verbatim=true&include_timestamps=true&keyterms=…`, header `xi-api-key`.
//! - Out: `{"message_type":"input_audio_chunk","audio_base_64":…,"commit":false,"sample_rate":16000}` per
//!   100 ms frame; `previous_text` only on a session's first chunk; an empty chunk with `commit:true` flushes.
//! - In: `session_started`; `partial_transcript {text}`; `committed_transcript {text}` followed ~30 ms later by
//!   `committed_transcript_with_timestamps {text, words[{text,start,end,type}]}` (seconds of session audio).
//!   Errors are messages (`auth_error`, `quota_exceeded`, `session_time_limit_exceeded`, …) then a close.
//!
//! Times: word times are seconds of what this session was sent. Each chunk
//! sent is anchored from session time to the side's timeline
//! ([`Anchors`]), so a replayed session still lands its words on the
//! meeting's clock. Turn prediction commits within ~36 s, well inside the
//! 60 s a side keeps for a replay.

use std::time::Duration;

use base64::Engine;
use futures::{SinkExt, StreamExt};
use recap_engine::{AudioFrame, LiveEvent, LiveOptions, LiveStatus, ProviderError, ProviderErrorKind, SAMPLE_RATE};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_tungstenite::tungstenite::{self, Message, client::IntoClientRequest, http::HeaderValue};

use super::super::keyterms;
use super::super::live_common::{self, Anchors, Failure, Retries, SideAudio, Sink, Ws};
use super::{Inner, http_error};

/// How much of the last finals goes along as `previous_text`.
const CONTEXT_CHARS: usize = 300;
/// A `committed_transcript` waits this long for its timed twin at the end.
const TWIN_WAIT: Duration = Duration::from_millis(300);
const KEEPALIVE_MS: u64 = 100;
/// How far behind the audio a turn's first partial comes (stt_results: seen
/// after ~0.7 s): where a partial's words likely began.
const PARTIAL_LAG_MS: u64 = 700;

/// Where a new partial starts: partials carry no word times, so not before
/// the last final's end (`heard_to`) and about the lag behind the audio sent
/// (`latest_ms`). With the last final's end alone, every next turn of the
/// side started where the last one ended and joined its row: the real app's
/// live transcript was one row for the whole meeting.
fn partial_start(heard_to: u64, latest_ms: u64) -> u64 {
    heard_to.max(latest_ms.saturating_sub(PARTIAL_LAG_MS))
}

fn network(message: impl std::fmt::Display) -> Failure {
    live_common::network("ElevenLabs", message)
}

/// A server message, as far as captions care.
enum Server {
    Started,
    Partial(String),
    Committed(String),
    /// A commit with its words' (start, end), in seconds of session audio.
    Timed(String, Vec<(f64, f64)>),
    Failed(Failure),
    Other,
}

fn parse(text: &str) -> Server {
    let Ok(message) = serde_json::from_str::<Value>(text) else { return Server::Other };
    let kind = message["message_type"].as_str().unwrap_or("");
    let text = || message["text"].as_str().unwrap_or("").trim().to_owned();
    let error = || {
        let detail = message["error"].as_str().or(message["message"].as_str()).unwrap_or(kind);
        format!("ElevenLabs {kind}: {detail}")
    };
    match kind {
        "session_started" => Server::Started,
        "partial_transcript" => Server::Partial(text()),
        "committed_transcript" => Server::Committed(text()),
        "committed_transcript_with_timestamps" => {
            let words = message["words"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            let words = words
                .iter()
                .filter(|w| w["type"].as_str().unwrap_or("word") == "word")
                .filter_map(|w| Some((w["start"].as_f64()?, w["end"].as_f64()?)))
                .collect();
            Server::Timed(text(), words)
        }
        "auth_error" | "unaccepted_terms" => {
            Server::Failed(Failure::Fatal(ProviderError::new(ProviderErrorKind::Auth, error())))
        }
        "quota_exceeded" => Server::Failed(Failure::Fatal(ProviderError::new(ProviderErrorKind::Quota, error()))),
        "input_error" | "invalid_request" | "chunk_size_exceeded" => {
            Server::Failed(Failure::Fatal(ProviderError::new(ProviderErrorKind::Protocol, error())))
        }
        // Only for manual commits; ours is the last one, so there is nothing to redo.
        "commit_throttled" => Server::Other,
        // session_time_limit_exceeded, rate_limited, queue_overflow, resource_exhausted,
        // transcriber_error, insufficient_audio_activity, and any error we don't know yet.
        _ if message.get("error").is_some() => {
            Server::Failed(Failure::Retry(ProviderError::new(ProviderErrorKind::Network, error())))
        }
        _ => Server::Other,
    }
}

fn url(inner: &Inner, options: &LiveOptions) -> String {
    let base = inner.config.realtime.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}/v1/speech-to-text/realtime")).expect("a realtime base URL");
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("model_id", "scribe_v3_live")
            .append_pair("audio_format", "pcm_16000")
            .append_pair("language_code", options.language.as_deref().unwrap_or("en"))
            .append_pair("commit_strategy", "turn_prediction")
            .append_pair("no_verbatim", "true")
            .append_pair("include_timestamps", "true");
        for term in keyterms(&options.keyterms) {
            query.append_pair("keyterms", &term);
        }
    }
    url.to_string()
}

async fn connect(inner: &Inner, url: &str) -> Result<Ws, Failure> {
    let mut request = url
        .into_client_request()
        .map_err(|e| Failure::Fatal(ProviderError::new(ProviderErrorKind::Protocol, e.to_string())))?;
    let key = HeaderValue::from_str(&inner.key).map_err(|_| {
        Failure::Fatal(ProviderError::new(ProviderErrorKind::Auth, "The ElevenLabs key has odd characters"))
    })?;
    request.headers_mut().insert("xi-api-key", key);
    match timeout(inner.config.connect_timeout, tokio_tungstenite::connect_async(request)).await {
        Err(_) => Err(network("timed out connecting")),
        Ok(Ok((ws, _))) => Ok(ws),
        Ok(Err(tungstenite::Error::Http(response))) => {
            let body = response.body().as_deref().map(String::from_utf8_lossy).unwrap_or_default();
            let error = http_error(response.status().as_u16(), &body);
            Err(if error.kind == ProviderErrorKind::Network { Failure::Retry(error) } else { Failure::Fatal(error) })
        }
        Ok(Err(e)) => Err(network(e)),
    }
}

/// The Test button: a session opens with this key.
pub(super) async fn check(inner: &Inner) -> Result<(), ProviderError> {
    let ws = connect(inner, &url(inner, &LiveOptions::default())).await.map_err(Failure::error)?;
    let (mut sink, mut stream) = ws.split();
    let started = async {
        while let Some(message) = stream.next().await {
            match message {
                Ok(Message::Text(text)) => match parse(&text) {
                    Server::Started => return Ok(()),
                    Server::Failed(failure) => return Err(failure.error()),
                    _ => {}
                },
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(e) => return Err(network(e).error()),
            }
        }
        Err(network("closed before the session started").error())
    };
    let result = timeout(inner.config.connect_timeout, started)
        .await
        .unwrap_or_else(|_| Err(network("no session_started in time").error()));
    let _ = timeout(Duration::from_secs(1), sink.send(Message::Close(None))).await;
    result
}

/// One connection's view: what it was sent, and context still to send.
struct Conn {
    anchors: Anchors,
    last_send: Instant,
    previous_text: Option<String>,
    /// A commit whose timed twin hasn't come yet.
    pending: Option<String>,
    /// It heard speech (a transcript), so its failure starts the retry count over.
    heard: bool,
}

impl Conn {
    fn new(previous_text: Option<String>) -> Self {
        Self { anchors: Anchors::default(), last_send: Instant::now(), previous_text, pending: None, heard: false }
    }

    async fn send_samples(
        &mut self,
        sink: &mut Sink,
        samples: &[i16],
        side: Option<(u64, u64)>,
        at: u64,
    ) -> Result<(), Failure> {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut message = json!({
            "message_type": "input_audio_chunk",
            "audio_base_64": base64::engine::general_purpose::STANDARD.encode(bytes),
            "commit": false,
            "sample_rate": SAMPLE_RATE,
        });
        if let Some(context) = self.previous_text.take() {
            message["previous_text"] = context.into();
        }
        sink.send(Message::text(message.to_string())).await.map_err(network)?;
        let session_len = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
        let (side_ms, side_len) = side.unwrap_or((at, 0));
        self.anchors.sent(session_len, side_ms, side_len);
        self.last_send = Instant::now();
        Ok(())
    }

    async fn send_frame(&mut self, sink: &mut Sink, frame: &AudioFrame) -> Result<(), Failure> {
        self.send_samples(sink, &frame.samples, Some((frame.start_ms, frame.duration_ms())), frame.start_ms).await
    }

    async fn commit(&mut self, sink: &mut Sink) -> Result<(), Failure> {
        let message = json!({"message_type": "input_audio_chunk", "audio_base_64": "", "commit": true, "sample_rate": SAMPLE_RATE});
        sink.send(Message::text(message.to_string())).await.map_err(network)
    }
}

/// One side's captions across connections.
struct Session {
    events: mpsc::UnboundedSender<LiveEvent>,
    side: SideAudio,
    /// The last finals' text, the next session's `previous_text`.
    context: String,
    /// The partial on screen, if any.
    open: Option<LiveEvent>,
}

impl Session {
    fn partial(&mut self, text: String) {
        if text.is_empty() || self.open.as_ref().is_some_and(|open| open.text == text) {
            return;
        }
        // The turn's start is set by its first partial and kept.
        let start_ms = match &self.open {
            Some(open) => open.start_ms,
            None => partial_start(self.side.heard_to.unwrap_or(0), self.side.latest_ms),
        };
        let event = LiveEvent { text, is_final: false, start_ms, end_ms: self.side.latest_ms.max(start_ms) };
        self.open = Some(event.clone());
        let _ = self.events.send(event);
    }

    fn final_(&mut self, conn: &mut Conn, text: String, words: &[(f64, f64)]) {
        if text.is_empty() {
            return;
        }
        let from = self.side.heard_to.unwrap_or(0);
        let (start_ms, end_ms) = match (words.first(), words.last()) {
            (Some(first), Some(last)) => {
                let start = conn.anchors.side_ms(first.0, from);
                (start, conn.anchors.side_ms(last.1, from).max(start))
            }
            _ => (from, self.side.latest_ms.max(from)),
        };
        if let Some(last) = words.last() {
            conn.anchors.forget_before(last.1);
        }
        self.side.said_to(end_ms);
        self.open = None;
        if !self.context.is_empty() {
            self.context.push(' ');
        }
        self.context.push_str(&text);
        if self.context.chars().count() > CONTEXT_CHARS {
            let skip = self.context.chars().count() - CONTEXT_CHARS;
            let cut: String = self.context.chars().skip(skip).collect();
            // Start at a word.
            self.context = match cut.split_once(' ') {
                Some((_, rest)) if !rest.is_empty() => rest.to_owned(),
                _ => cut,
            };
        }
        let _ = self.events.send(LiveEvent { text, is_final: true, start_ms, end_ms });
    }

    /// A commit that never got its timed twin: final with estimated times.
    fn settle(&mut self, conn: &mut Conn) {
        if let Some(text) = conn.pending.take() {
            self.final_(conn, text, &[]);
        }
    }

    /// The audio is over: what is still partial is what was said.
    fn close_open(&mut self) {
        if let Some(open) = self.open.take() {
            let _ = self.events.send(LiveEvent { is_final: true, ..open });
        }
    }

    /// One connection, until the audio has closed and been flushed (Ok) or
    /// it fails.
    async fn connection(
        &mut self,
        inner: &Inner,
        ws: Ws,
        audio: &mut mpsc::UnboundedReceiver<AudioFrame>,
        conn: &mut Conn,
    ) -> Result<(), Failure> {
        let (mut sink, mut stream) = ws.split();
        let result = self.pump(inner, &mut sink, &mut stream, audio, conn).await;
        if result.is_ok() {
            let _ = timeout(Duration::from_secs(1), sink.send(Message::Close(None))).await;
        }
        result
    }

    async fn pump(
        &mut self,
        inner: &Inner,
        sink: &mut Sink,
        stream: &mut futures::stream::SplitStream<Ws>,
        audio: &mut mpsc::UnboundedReceiver<AudioFrame>,
        conn: &mut Conn,
    ) -> Result<(), Failure> {
        for frame in &self.side.replay() {
            conn.send_frame(sink, frame).await?;
        }
        // When the flush ends, once the audio has closed.
        let mut flush: Option<Instant> = None;
        if !self.side.open {
            conn.commit(sink).await?;
            flush = Some(Instant::now() + inner.config.flush);
        }
        loop {
            let keepalive = conn.last_send + inner.config.keepalive;
            let flush_at = flush.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
            tokio::select! {
                message = stream.next() => match message {
                    Some(Ok(Message::Text(text))) => match parse(&text) {
                        Server::Partial(text) => {
                            conn.heard = true;
                            self.settle(conn);
                            self.partial(text);
                        }
                        Server::Committed(text) => {
                            conn.heard = true;
                            self.settle(conn);
                            conn.pending = Some(text);
                            if let Some(at) = flush.as_mut() {
                                *at = (*at).min(Instant::now() + TWIN_WAIT);
                            }
                        }
                        Server::Timed(text, words) => {
                            conn.heard = true;
                            conn.pending = None;
                            self.final_(conn, text, &words);
                            if flush.is_some() {
                                break;
                            }
                        }
                        Server::Failed(failure) => return Err(failure),
                        Server::Started | Server::Other => {}
                    },
                    Some(Ok(Message::Close(_))) | None => return Err(network("the connection closed")),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(network(e)),
                },
                frame = audio.recv(), if self.side.open => match frame {
                    Some(frame) => {
                        self.side.keep(&frame);
                        conn.send_frame(sink, &frame).await?;
                    }
                    None => {
                        self.side.open = false;
                        conn.commit(sink).await?;
                        flush = Some(Instant::now() + inner.config.flush);
                    }
                },
                _ = sleep_until(keepalive), if self.side.open => {
                    let silence = vec![0i16; (SAMPLE_RATE as u64 * KEEPALIVE_MS / 1000) as usize];
                    conn.send_samples(sink, &silence, None, self.side.latest_ms).await?;
                }
                _ = sleep_until(flush_at), if flush.is_some() => break,
            }
        }
        self.settle(conn);
        self.close_open();
        Ok(())
    }
}

pub(super) async fn run(
    inner: std::sync::Arc<Inner>,
    options: LiveOptions,
    mut audio: mpsc::UnboundedReceiver<AudioFrame>,
    events: mpsc::UnboundedSender<LiveEvent>,
    status: mpsc::UnboundedSender<LiveStatus>,
) -> Result<(), ProviderError> {
    let url = url(&inner, &options);
    // A connection was lost and none has come back yet.
    let mut reconnecting = false;
    let mut session = Session { events, side: SideAudio::new(), context: String::new(), open: None };
    let mut retries = Retries::default();
    let mut first = true;
    loop {
        // Disconnected when the audio closed: no reconnect just to flush.
        if !session.side.open {
            session.close_open();
            return Ok(());
        }
        let previous_text = (!first && !session.context.is_empty()).then(|| session.context.clone());
        first = false;
        let mut conn = Conn::new(previous_text);
        let result = match connect(&inner, &url).await {
            Ok(ws) => {
                if std::mem::take(&mut reconnecting) {
                    let _ = status.send(LiveStatus::Connected);
                }
                session.connection(&inner, ws, &mut audio, &mut conn).await
            }
            Err(failure) => Err(failure),
        };
        let error = match result {
            Ok(()) => return Ok(()),
            Err(Failure::Fatal(error)) => return Err(error),
            Err(Failure::Retry(error)) => error,
        };
        if !reconnecting && session.side.open {
            reconnecting = true;
            let _ = status.send(LiveStatus::Reconnecting);
        }
        let wait = retries.after(conn.heard, error, inner.config.retries, inner.config.backoff)?;
        session.side.back_off(wait, &mut audio).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_errors_are_typed() {
        let kind = |t: &str| match parse(&json!({"message_type": t, "error": "x"}).to_string()) {
            Server::Failed(Failure::Fatal(e)) => format!("fatal {:?}", e.kind),
            Server::Failed(Failure::Retry(e)) => format!("retry {:?}", e.kind),
            _ => "other".into(),
        };
        assert_eq!(kind("auth_error"), "fatal Auth");
        assert_eq!(kind("unaccepted_terms"), "fatal Auth");
        assert_eq!(kind("quota_exceeded"), "fatal Quota");
        assert_eq!(kind("session_time_limit_exceeded"), "retry Network");
        assert_eq!(kind("rate_limited"), "retry Network");
        assert_eq!(kind("input_error"), "fatal Protocol");
        assert_eq!(kind("commit_throttled"), "other");
    }

    #[test]
    fn a_new_partial_starts_about_the_lag_behind_the_audio_not_at_the_last_final() {
        // The test meeting: Tom's turn ended at 9.4 s, Priya began at 11.04 s,
        // her first partial came with ~11.9 s of audio sent.
        let start = partial_start(9_400, 11_900);
        assert_eq!(start, 11_200);
        assert!(start > 9_400 + 400, "past the transcript's join gap: a row of its own");
        // Speech that runs on through a commit: the partial follows at once.
        assert_eq!(partial_start(9_400, 9_900), 9_400);
        assert_eq!(partial_start(0, 300), 0);
    }
}
