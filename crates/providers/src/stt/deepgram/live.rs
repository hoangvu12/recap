//! Live captions over Deepgram's streaming endpoint.
//!
//! Protocol (Deepgram's docs; live_captions.py):
//! - `GET /v1/listen?model=nova-3&language=en&encoding=linear16&sample_rate=16000&channels=1
//!   &interim_results=true&smart_format=true&endpointing=300&utterance_end_ms=1000&keyterm=…`,
//!   header `Authorization: Token <key>`. A refused upgrade is an HTTP error with
//!   `{err_code, err_msg}` (401 key, 402 `ASR_PAYMENT_REQUIRED`).
//! - Out: audio as binary frames; text `{"type":"KeepAlive"}` while there is
//!   none, `{"type":"CloseStream"}` at the end.
//! - In: `Results {is_final, speech_final, channel.alternatives[0].{transcript, words[{start,end}]}}`
//!   (seconds of the audio this stream was sent); `UtteranceEnd`; `Metadata`
//!   after CloseStream, then a close. Errors close the socket: `1011 NET-…`
//!   (server or timeout: reconnect), `1008 DATA-…` (our message: give up).
//!
//! Captions: interim results are partials, shown after the finals of the
//! same utterance; finals join until `speech_final` or `UtteranceEnd` (or a
//! long monologue) and then go out as one final.

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use recap_engine::{AudioFrame, LiveEvent, LiveOptions, ProviderError, ProviderErrorKind, SAMPLE_RATE};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_tungstenite::tungstenite::{
    self, Message,
    client::IntoClientRequest,
    http::HeaderValue,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

use super::super::keyterms;
use super::super::live_common::{self, Anchors, Failure, Retries, SideAudio, Sink, Ws};
use super::{Inner, http_error};

/// A monologue goes out as a final at about this many characters, even
/// without a pause (live_captions.py's COMMIT_CHARS).
const COMMIT_CHARS: usize = 220;

fn network(message: impl std::fmt::Display) -> Failure {
    live_common::network("Deepgram", message)
}

/// An error told in words (an `Error` message, a close reason): no credit
/// and a refused key end the session, the rest is worth a reconnect.
fn classify(text: &str) -> Failure {
    let lower = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if has(&["credit", "payment", "balance", "quota"]) {
        Failure::Fatal(ProviderError::new(ProviderErrorKind::Quota, format!("Deepgram quota: {text}")))
    } else if has(&["auth", "credential", "api key", "forbidden"]) {
        Failure::Fatal(ProviderError::new(ProviderErrorKind::Auth, format!("Deepgram refused the key: {text}")))
    } else {
        network(text)
    }
}

/// The server closed the stream while we were still sending.
fn closed(frame: Option<CloseFrame>) -> Failure {
    let Some(frame) = frame else { return network("the connection closed") };
    let reason = frame.reason.as_str().to_owned();
    let told = format!("{} {reason}", u16::from(frame.code));
    match classify(&reason) {
        Failure::Retry(_) => match frame.code {
            // Our message was wrong (DATA-…, BIG-…): sending it again won't help.
            CloseCode::Policy | CloseCode::Unsupported | CloseCode::Size => Failure::Fatal(ProviderError::new(
                ProviderErrorKind::Protocol,
                format!("Deepgram closed the stream: {told}"),
            )),
            _ => network(format!("closed the stream ({told})")),
        },
        fatal => fatal,
    }
}

enum Server {
    Results { text: String, words: Vec<(f64, f64)>, is_final: bool, speech_final: bool },
    UtteranceEnd,
    Metadata,
    Failed(Failure),
    Other,
}

fn parse(text: &str) -> Server {
    let Ok(message) = serde_json::from_str::<Value>(text) else { return Server::Other };
    match message["type"].as_str().unwrap_or("") {
        "Results" => {
            let alternative = &message["channel"]["alternatives"][0];
            let words = alternative["words"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            Server::Results {
                text: alternative["transcript"].as_str().unwrap_or("").trim().to_owned(),
                words: words.iter().filter_map(|w| Some((w["start"].as_f64()?, w["end"].as_f64()?))).collect(),
                is_final: message["is_final"].as_bool().unwrap_or(false),
                speech_final: message["speech_final"].as_bool().unwrap_or(false),
            }
        }
        "UtteranceEnd" => Server::UtteranceEnd,
        "Metadata" => Server::Metadata,
        kind if kind == "Error" || message.get("err_code").is_some() => {
            let told: Vec<&str> = ["description", "message", "err_msg", "variant", "err_code"]
                .iter()
                .filter_map(|k| message[*k].as_str())
                .filter(|s| !s.is_empty())
                .collect();
            Server::Failed(classify(&if told.is_empty() { "an error".to_owned() } else { told.join(": ") }))
        }
        _ => Server::Other,
    }
}

fn url(inner: &Inner, options: &LiveOptions) -> String {
    let base = inner.config.live.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}/v1/listen")).expect("a live base URL");
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("model", "nova-3")
            // Auto: nova-3's multilingual code-switching (ADR 0022).
            .append_pair("language", recap_engine::language_code(options.language.as_deref()).unwrap_or("multi"))
            .append_pair("encoding", "linear16")
            .append_pair("sample_rate", &SAMPLE_RATE.to_string())
            .append_pair("channels", "1")
            .append_pair("interim_results", "true")
            .append_pair("smart_format", "true")
            .append_pair("endpointing", "300")
            .append_pair("utterance_end_ms", "1000");
        for term in keyterms(&options.keyterms) {
            query.append_pair("keyterm", &term);
        }
    }
    url.to_string()
}

async fn connect(inner: &Inner, url: &str) -> Result<Ws, Failure> {
    let mut request = url
        .into_client_request()
        .map_err(|e| Failure::Fatal(ProviderError::new(ProviderErrorKind::Protocol, e.to_string())))?;
    let token = HeaderValue::from_str(&format!("Token {}", inner.key)).map_err(|_| {
        Failure::Fatal(ProviderError::new(ProviderErrorKind::Auth, "The Deepgram key has odd characters"))
    })?;
    request.headers_mut().insert("authorization", token);
    match timeout(inner.config.connect_timeout, tokio_tungstenite::connect_async(request)).await {
        Err(_) => Err(network("timed out connecting")),
        Ok(Ok((ws, _))) => Ok(ws),
        Ok(Err(tungstenite::Error::Http(response))) => {
            let mut body = response.body().as_deref().map(String::from_utf8_lossy).unwrap_or_default().into_owned();
            if body.trim().is_empty()
                && let Some(told) = response.headers().get("dg-error").and_then(|v| v.to_str().ok())
            {
                body = json!({ "err_msg": told }).to_string();
            }
            let error = http_error(response.status().as_u16(), &body);
            Err(if error.kind == ProviderErrorKind::Network { Failure::Retry(error) } else { Failure::Fatal(error) })
        }
        Ok(Err(e)) => Err(network(e)),
    }
}

fn close_stream() -> Message {
    Message::text(json!({"type": "CloseStream"}).to_string())
}

/// The Test button: a stream opens with this key (no audio is sent, so it
/// costs nothing).
pub(super) async fn check(inner: &Inner) -> Result<(), ProviderError> {
    let ws = connect(inner, &url(inner, &LiveOptions::default())).await.map_err(Failure::error)?;
    let (mut sink, mut stream) = ws.split();
    let _ = sink.send(close_stream()).await;
    let _ = timeout(Duration::from_secs(1), async {
        while let Some(Ok(message)) = stream.next().await {
            if message.is_close() {
                break;
            }
        }
    })
    .await;
    Ok(())
}

/// One connection's view: the audio it was sent.
struct Conn {
    anchors: Anchors,
    last_send: Instant,
    /// It heard speech, so its failure starts the retry count over.
    heard: bool,
}

impl Conn {
    fn new() -> Self {
        Self { anchors: Anchors::default(), last_send: Instant::now(), heard: false }
    }

    async fn send_frame(&mut self, sink: &mut Sink, frame: &AudioFrame) -> Result<(), Failure> {
        let bytes: Vec<u8> = frame.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        sink.send(Message::binary(bytes)).await.map_err(network)?;
        let len = frame.samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
        // Keepalives are text: every millisecond sent is the side's audio.
        self.anchors.sent(len, frame.start_ms, len);
        self.last_send = Instant::now();
        Ok(())
    }

    async fn keepalive(&mut self, sink: &mut Sink) -> Result<(), Failure> {
        sink.send(Message::text(json!({"type": "KeepAlive"}).to_string())).await.map_err(network)?;
        self.last_send = Instant::now();
        Ok(())
    }
}

/// A final of an utterance not over yet.
struct Piece {
    text: String,
    start_ms: u64,
    end_ms: u64,
    /// Its last word's end, in stream seconds.
    stream_end: f64,
}

/// One side's captions across connections.
struct Session {
    events: mpsc::UnboundedSender<LiveEvent>,
    side: SideAudio,
    /// The utterance's finals so far.
    pending: Vec<Piece>,
    /// The partial on screen, if any.
    open: Option<LiveEvent>,
}

impl Session {
    fn pending_text(&self) -> String {
        self.pending.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    fn results(&mut self, conn: &mut Conn, text: String, words: &[(f64, f64)], is_final: bool, speech_final: bool) {
        if !text.is_empty() {
            conn.heard = true;
        }
        let from = self.side.heard_to.unwrap_or(0);
        let span = match (words.first(), words.last()) {
            (Some(first), Some(last)) => {
                let start = conn.anchors.side_ms(first.0, from);
                Some((start, conn.anchors.side_ms(last.1, from).max(start), last.1))
            }
            _ => None,
        };
        if is_final {
            if !text.is_empty() {
                let (start_ms, end_ms, stream_end) = span.unwrap_or((from, self.side.latest_ms.max(from), 0.0));
                self.pending.push(Piece { text, start_ms, end_ms, stream_end });
            }
            if speech_final || self.pending_text().chars().count() > COMMIT_CHARS {
                self.commit(conn);
            } else if !self.pending.is_empty() {
                self.partial(self.pending_text(), None);
            }
        } else if !text.is_empty() || !self.pending.is_empty() {
            let shown = [self.pending_text(), text].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
            self.partial(shown, span.map(|(s, e, _)| (s, e)));
        }
    }

    fn partial(&mut self, text: String, span: Option<(u64, u64)>) {
        // Flushing: the finals are on their way, nobody reads a partial now.
        if !self.side.open || text.is_empty() || self.open.as_ref().is_some_and(|open| open.text == text) {
            return;
        }
        let start_ms =
            self.pending.first().map(|p| p.start_ms).or(span.map(|s| s.0)).unwrap_or(self.side.heard_to.unwrap_or(0));
        let end_ms =
            span.map(|s| s.1).or(self.pending.last().map(|p| p.end_ms)).unwrap_or(self.side.latest_ms).max(start_ms);
        let event = LiveEvent { text, is_final: false, start_ms, end_ms };
        self.open = Some(event.clone());
        let _ = self.events.send(event);
    }

    /// The utterance's finals go out as one final.
    fn commit(&mut self, conn: &mut Conn) {
        if self.pending.is_empty() {
            return;
        }
        let text = self.pending_text();
        let start_ms = self.pending[0].start_ms;
        let end_ms = self.pending.iter().map(|p| p.end_ms).max().unwrap_or(start_ms).max(start_ms);
        let stream_end = self.pending.iter().map(|p| p.stream_end).fold(0.0, f64::max);
        self.pending.clear();
        conn.anchors.forget_before(stream_end);
        self.final_(LiveEvent { text, is_final: true, start_ms, end_ms });
    }

    fn final_(&mut self, event: LiveEvent) {
        self.side.said_to(event.end_ms);
        self.open = None;
        let _ = self.events.send(event);
    }

    /// The audio is over: what was heard is what was said. A partial that
    /// already holds the finals (and more) wins; else the finals do.
    fn finish(&mut self, conn: &mut Conn) {
        let pending = self.pending_text();
        match self.open.take() {
            Some(open) if pending.is_empty() || open.text.starts_with(&pending) => {
                self.pending.clear();
                self.final_(LiveEvent { is_final: true, ..open });
            }
            _ => self.commit(conn),
        }
    }

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
            sink.send(close_stream()).await.map_err(network)?;
            flush = Some(Instant::now() + inner.config.flush);
        }
        loop {
            let keepalive = conn.last_send + inner.config.keepalive;
            let flush_at = flush.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
            tokio::select! {
                message = stream.next() => match message {
                    Some(Ok(Message::Text(text))) => match parse(&text) {
                        Server::Results { text, words, is_final, speech_final } => {
                            self.results(conn, text, &words, is_final, speech_final);
                        }
                        Server::UtteranceEnd => self.commit(conn),
                        // After CloseStream: every result is in.
                        Server::Metadata if flush.is_some() => break,
                        Server::Failed(failure) => return Err(failure),
                        Server::Metadata | Server::Other => {}
                    },
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) if flush.is_some() => break,
                    Some(Ok(Message::Close(frame))) => return Err(closed(frame)),
                    None => return Err(network("the connection closed")),
                    Some(Err(e)) => return Err(network(e)),
                    Some(Ok(_)) => {}
                },
                frame = audio.recv(), if self.side.open => match frame {
                    Some(frame) => {
                        self.side.keep(&frame);
                        conn.send_frame(sink, &frame).await?;
                    }
                    None => {
                        self.side.open = false;
                        sink.send(close_stream()).await.map_err(network)?;
                        flush = Some(Instant::now() + inner.config.flush);
                    }
                },
                _ = sleep_until(keepalive), if self.side.open => conn.keepalive(sink).await?,
                _ = sleep_until(flush_at), if flush.is_some() => break,
            }
        }
        self.finish(conn);
        Ok(())
    }
}

pub(super) async fn run(
    inner: std::sync::Arc<Inner>,
    options: LiveOptions,
    mut audio: mpsc::UnboundedReceiver<AudioFrame>,
    events: mpsc::UnboundedSender<LiveEvent>,
) -> Result<(), ProviderError> {
    let url = url(&inner, &options);
    let mut session = Session { events, side: SideAudio::new(), pending: Vec::new(), open: None };
    let mut retries = Retries::default();
    loop {
        // Disconnected when the audio closed: no reconnect just to flush.
        if !session.side.open {
            let mut conn = Conn::new();
            session.finish(&mut conn);
            return Ok(());
        }
        let mut conn = Conn::new();
        let result = match connect(&inner, &url).await {
            Ok(ws) => session.connection(&inner, ws, &mut audio, &mut conn).await,
            Err(failure) => Err(failure),
        };
        let error = match result {
            Ok(()) => return Ok(()),
            Err(Failure::Fatal(error)) => return Err(error),
            Err(Failure::Retry(error)) => error,
        };
        // What was final before the drop stays said; the rest is replayed.
        session.commit(&mut conn);
        let wait = retries.after(conn.heard, error, inner.config.retries, inner.config.backoff)?;
        session.side.back_off(wait, &mut audio).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(failure: Failure) -> String {
        match failure {
            Failure::Fatal(e) => format!("fatal {:?}", e.kind),
            Failure::Retry(e) => format!("retry {:?}", e.kind),
        }
    }

    #[test]
    fn closes_and_errors_are_typed() {
        let close = |code: u16, reason: &'static str| {
            kind(closed(Some(CloseFrame { code: code.into(), reason: tungstenite::Utf8Bytes::from_static(reason) })))
        };
        assert_eq!(close(1011, "NET-0001"), "retry Network");
        assert_eq!(close(1011, "NET-0000"), "retry Network");
        assert_eq!(close(1008, "DATA-0000"), "fatal Protocol");
        assert_eq!(close(1009, "BIG-0000"), "fatal Protocol");
        assert_eq!(close(1008, "Insufficient credits"), "fatal Quota");
        assert_eq!(kind(closed(None)), "retry Network");
        let error = |v: Value| match parse(&v.to_string()) {
            Server::Failed(f) => kind(f),
            _ => "other".into(),
        };
        assert_eq!(
            error(json!({"type": "Error", "description": "Project does not have enough credits"})),
            "fatal Quota"
        );
        assert_eq!(error(json!({"type": "Error", "description": "Invalid credentials."})), "fatal Auth");
        assert_eq!(
            error(json!({"type": "Error", "description": "Internal error", "variant": "NET-0000"})),
            "retry Network"
        );
        assert_eq!(error(json!({"type": "SpeechStarted"})), "other");
    }
}
