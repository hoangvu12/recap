//! A local fake of Deepgram's live endpoint (`/v1/listen` as a websocket):
//! a script per connection (messages in Deepgram's documented shapes, waits
//! on the client's audio or control messages, closes and drops). It records
//! what the client sent: audio as binary frames, control messages as text.
//! The pre-recorded endpoint is plain HTTP: [`super::FakeApi`].

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::{
    Message, Utf8Bytes,
    handshake::server::{ErrorResponse, Request, Response},
    http::StatusCode,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

#[derive(Clone, Debug)]
pub enum Step {
    /// Refuse the upgrade with this HTTP status and Deepgram's error body.
    Reject(u16, &'static str),
    Send(Value),
    /// Wait until the client has sent this many audio frames on this connection.
    AwaitAudio(usize),
    /// Wait for the client's control message of this type (`CloseStream`…).
    AwaitText(&'static str),
    /// Close with this code and reason (`1011`, `"NET-0001"`).
    Close(u16, &'static str),
    /// Drop the TCP connection, no close frame.
    Drop,
}

#[derive(Clone, Debug, Default)]
pub struct Conn {
    pub uri: String,
    pub authorization: Option<String>,
    /// The audio frames, decoded to samples.
    pub audio: Vec<Vec<i16>>,
    /// The control (text) messages.
    pub texts: Vec<Value>,
}

impl Conn {
    pub fn query(&self, name: &str) -> Vec<String> {
        let query = self.uri.split_once('?').map(|(_, q)| q).unwrap_or("");
        reqwest::Url::parse(&format!("http://x/?{query}"))
            .unwrap()
            .query_pairs()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
            .collect()
    }

    pub fn sent(&self, kind: &str) -> usize {
        self.texts.iter().filter(|t| t["type"] == kind).count()
    }
}

pub struct FakeListen {
    /// `ws://127.0.0.1:port`, the adapter's live base.
    pub base: String,
    conns: Arc<Mutex<Vec<Conn>>>,
}

impl FakeListen {
    pub async fn start(scripts: Vec<Vec<Step>>) -> FakeListen {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("ws://{}", listener.local_addr().unwrap());
        let conns = Arc::new(Mutex::new(Vec::new()));
        let all = conns.clone();
        tokio::spawn(async move {
            let mut scripts = scripts.into_iter();
            while let Ok((tcp, _)) = listener.accept().await {
                let Some(script) = scripts.next() else { break };
                let index = {
                    let mut conns = all.lock().unwrap();
                    conns.push(Conn::default());
                    conns.len() - 1
                };
                tokio::spawn(serve(tcp, script, all.clone(), index));
            }
        });
        FakeListen { base, conns }
    }

    pub fn conns(&self) -> Vec<Conn> {
        self.conns.lock().unwrap().clone()
    }
}

async fn serve(tcp: tokio::net::TcpStream, script: Vec<Step>, conns: Arc<Mutex<Vec<Conn>>>, index: usize) {
    let reject = match script.first() {
        Some(Step::Reject(status, body)) => Some((*status, *body)),
        _ => None,
    };
    let record = conns.clone();
    // tungstenite's handshake callback: its error type is theirs.
    #[allow(clippy::result_large_err)]
    let callback = move |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let mut conns = record.lock().unwrap();
        conns[index].uri = request.uri().to_string();
        conns[index].authorization = request.headers().get("authorization").map(|v| v.to_str().unwrap().to_owned());
        match reject {
            Some((status, body)) => {
                let mut error = ErrorResponse::new(Some(body.into()));
                *error.status_mut() = StatusCode::from_u16(status).unwrap();
                Err(error)
            }
            None => Ok(response),
        }
    };
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else { return };
    let (mut sink, mut stream) = ws.split();
    // (audio frames, control types) seen so far.
    let (seen_tx, mut seen) = watch::channel((0usize, Vec::<String>::new()));
    let reader_conns = conns.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(message)) = stream.next().await {
            match message {
                Message::Binary(bytes) => {
                    let samples = bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
                    reader_conns.lock().unwrap()[index].audio.push(samples);
                    seen_tx.send_modify(|(n, _)| *n += 1);
                }
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    let kind = value["type"].as_str().unwrap_or("").to_owned();
                    reader_conns.lock().unwrap()[index].texts.push(value);
                    seen_tx.send_modify(|(_, kinds)| kinds.push(kind));
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });
    for step in script {
        match step {
            Step::Reject(..) => unreachable!("handled at the handshake"),
            Step::Send(value) => {
                if sink.send(Message::text(value.to_string())).await.is_err() {
                    return;
                }
            }
            Step::AwaitAudio(n) => {
                if seen.wait_for(|(frames, _)| *frames >= n).await.is_err() {
                    return;
                }
            }
            Step::AwaitText(kind) => {
                if seen.wait_for(|(_, kinds)| kinds.iter().any(|k| k == kind)).await.is_err() {
                    return;
                }
            }
            Step::Close(code, reason) => {
                let frame = CloseFrame { code: CloseCode::from(code), reason: Utf8Bytes::from_static(reason) };
                let _ = sink.send(Message::Close(Some(frame))).await;
                let _ = tokio::time::timeout(Duration::from_secs(2), reader).await;
                return;
            }
            Step::Drop => {
                reader.abort();
                let _ = reader.await;
                drop(sink);
                return;
            }
        }
    }
    let _ = reader.await;
}

/// A `Results` message: `words` are (punctuated word, start, end) in
/// seconds of the audio this connection was sent.
pub fn results(words: &[(&str, f64, f64)], is_final: bool, speech_final: bool) -> Step {
    let list: Vec<Value> = words
        .iter()
        .map(|(w, s, e)| {
            json!({"word": w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()), "start": s, "end": e,
                   "confidence": 0.99, "punctuated_word": w})
        })
        .collect();
    let transcript: Vec<&str> = words.iter().map(|(w, _, _)| *w).collect();
    let start = words.first().map_or(0.0, |w| w.1);
    let end = words.last().map_or(start, |w| w.2);
    Step::Send(json!({
        "type": "Results", "channel_index": [0, 1], "duration": end - start, "start": start,
        "is_final": is_final, "speech_final": speech_final,
        "channel": {"alternatives": [{"transcript": transcript.join(" "), "confidence": 0.99, "words": list}]},
        "metadata": {"request_id": "fake", "model_info": {"name": "general-nova-3", "version": "fake", "arch": "nova-3"}},
        "from_finalize": false
    }))
}

pub fn interim(words: &[(&str, f64, f64)]) -> Step {
    results(words, false, false)
}

pub fn utterance_end(last_word_end: f64) -> Step {
    Step::Send(json!({"type": "UtteranceEnd", "channel": [0, 1], "last_word_end": last_word_end}))
}

/// What Deepgram sends after `CloseStream`, before it closes.
pub fn metadata() -> Step {
    Step::Send(json!({"type": "Metadata", "transaction_key": "deprecated", "request_id": "fake",
                      "sha256": "0", "created": "2026-11-17T09:00:00.000Z", "duration": 1.0, "channels": 1}))
}

/// `fixtures/deepgram_live.jsonl` (Deepgram's documented messages over the
/// test meeting's first two turns) as a script: each message waits for the
/// audio it was heard after; then `CloseStream` ends it.
pub fn recorded_session() -> Vec<Step> {
    let mut steps = Vec::new();
    for line in include_str!("../fixtures/deepgram_live.jsonl").lines().filter(|l| !l.trim().is_empty()) {
        let entry: Value = serde_json::from_str(line).unwrap();
        let t = entry["t"].as_f64().unwrap();
        steps.push(Step::AwaitAudio((t * 10.0).ceil() as usize));
        steps.push(Step::Send(entry["msg"].clone()));
    }
    steps.extend([Step::AwaitText("CloseStream"), metadata(), Step::Close(1000, "")]);
    steps
}
