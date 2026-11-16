//! Local fakes of ElevenLabs for the adapter's protocol tests: a websocket
//! server that plays a script per connection (recorded messages, waits on
//! the client's audio, drops) and an HTTP server that answers one canned
//! response per request. Both record what the client sent.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::{
    Message,
    handshake::server::{ErrorResponse, Request, Response},
    http::StatusCode,
};

/// One step of a connection's script.
#[derive(Clone, Debug)]
pub enum Step {
    /// Refuse the websocket handshake with this HTTP status.
    Reject(u16),
    /// Send a server message.
    Send(Value),
    /// Wait until the client has sent this many audio chunks on this
    /// connection (empty commit chunks don't count).
    AwaitChunks(usize),
    /// Wait for the client's `commit: true`.
    AwaitCommit,
    /// Close the websocket cleanly.
    Close,
    /// Drop the TCP connection, no close frame: a network blip.
    Drop,
}

/// What one client connection did.
#[derive(Clone, Debug, Default)]
pub struct Conn {
    /// Path and query of the upgrade request.
    pub uri: String,
    pub key: Option<String>,
    /// Every message the client sent, parsed.
    pub sent: Vec<Value>,
}

impl Conn {
    /// The query's values for `name`, decoded.
    pub fn query(&self, name: &str) -> Vec<String> {
        let query = self.uri.split_once('?').map(|(_, q)| q).unwrap_or("");
        reqwest::Url::parse(&format!("http://x/?{query}"))
            .unwrap()
            .query_pairs()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
            .collect()
    }

    /// The audio chunks it sent, decoded to samples (commits left out).
    pub fn chunks(&self) -> Vec<Vec<i16>> {
        self.sent
            .iter()
            .filter(|m| m["message_type"] == "input_audio_chunk")
            .map(|m| decode(m["audio_base_64"].as_str().unwrap_or("")))
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn committed(&self) -> bool {
        self.sent.iter().any(|m| m["commit"] == true)
    }
}

pub fn decode(b64: &str) -> Vec<i16> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
}

/// A fake realtime endpoint: connection `n` plays `scripts[n]`; once the
/// script is done the connection stays open until the client closes it.
pub struct FakeScribe {
    /// `ws://127.0.0.1:port`, the adapter's realtime base.
    pub base: String,
    conns: Arc<Mutex<Vec<Conn>>>,
}

impl FakeScribe {
    pub async fn start(scripts: Vec<Vec<Step>>) -> FakeScribe {
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
        FakeScribe { base, conns }
    }

    pub fn conns(&self) -> Vec<Conn> {
        self.conns.lock().unwrap().clone()
    }
}

async fn serve(tcp: tokio::net::TcpStream, script: Vec<Step>, conns: Arc<Mutex<Vec<Conn>>>, index: usize) {
    let reject = match script.first() {
        Some(Step::Reject(status)) => Some(*status),
        _ => None,
    };
    let record = conns.clone();
    let callback = move |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let mut conns = record.lock().unwrap();
        conns[index].uri = request.uri().to_string();
        conns[index].key = request.headers().get("xi-api-key").map(|v| v.to_str().unwrap().to_owned());
        match reject {
            Some(status) => {
                let mut error = ErrorResponse::new(Some(r#"{"detail":{"status":"invalid_api_key"}}"#.into()));
                *error.status_mut() = StatusCode::from_u16(status).unwrap();
                Err(error)
            }
            None => Ok(response),
        }
    };
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else { return };
    let (mut sink, mut stream) = ws.split();
    // (chunks, committed) seen so far on this connection.
    let (seen_tx, mut seen) = watch::channel((0usize, false));
    let reader_conns = conns.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(message)) = stream.next().await {
            let Message::Text(text) = message else {
                if message.is_close() {
                    break;
                }
                continue;
            };
            let value: Value = serde_json::from_str(&text).unwrap();
            let is_chunk = value["message_type"] == "input_audio_chunk"
                && !value["audio_base_64"].as_str().unwrap_or("").is_empty();
            let commit = value["commit"] == true;
            reader_conns.lock().unwrap()[index].sent.push(value);
            seen_tx.send_modify(|(chunks, committed)| {
                *chunks += is_chunk as usize;
                *committed |= commit;
            });
        }
    });
    for step in script {
        match step {
            Step::Reject(_) => unreachable!("handled at the handshake"),
            Step::Send(value) => {
                if sink.send(Message::text(value.to_string())).await.is_err() {
                    return;
                }
            }
            Step::AwaitChunks(n) => {
                if seen.wait_for(|(chunks, _)| *chunks >= n).await.is_err() {
                    return;
                }
            }
            Step::AwaitCommit => {
                if seen.wait_for(|(_, committed)| *committed).await.is_err() {
                    return;
                }
            }
            Step::Close => {
                let _ = sink.send(Message::Close(None)).await;
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

/// `fixtures/elevenlabs_live.jsonl` (a real session on the test meeting) as
/// a script: each message waits for the audio it was heard after.
pub fn recorded_session() -> Vec<Step> {
    let text = include_str!("../fixtures/elevenlabs_live.jsonl");
    let mut steps = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let entry: Value = serde_json::from_str(line).unwrap();
        let t = entry["t"].as_f64().unwrap();
        steps.push(Step::AwaitChunks((t * 10.0).ceil() as usize));
        steps.push(Step::Send(entry["msg"].clone()));
    }
    steps
}

pub fn recorded_messages() -> Vec<Value> {
    include_str!("../fixtures/elevenlabs_live.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).unwrap()["msg"].clone())
        .collect()
}

pub fn started() -> Step {
    Step::Send(serde_json::json!({"message_type": "session_started", "session_id": "fake", "config": {}}))
}

/// A committed turn with its words (start, end in seconds of session audio).
pub fn committed(text: &str, words: &[(&str, f64, f64)]) -> Vec<Step> {
    let words: Vec<Value> = words
        .iter()
        .map(|(w, s, e)| serde_json::json!({"text": w, "start": s, "end": e, "type": "word", "speaker_id": null}))
        .collect();
    vec![
        Step::Send(serde_json::json!({"message_type": "committed_transcript", "text": text})),
        Step::Send(serde_json::json!({
            "message_type": "committed_transcript_with_timestamps", "text": text, "language_code": null, "words": words
        })),
    ]
}

pub fn partial(text: &str) -> Step {
    Step::Send(serde_json::json!({"message_type": "partial_transcript", "text": text, "turn_probability": 0.2}))
}

pub fn error(kind: &str) -> Step {
    Step::Send(serde_json::json!({"message_type": kind, "error": format!("{kind} (fake)")}))
}

/// One HTTP request the fake API received.
#[derive(Clone, Debug, Default)]
pub struct HttpRequest {
    pub head: String,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn header(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim().to_owned())
        })
    }

    /// A multipart form's text fields, in order (repeats kept).
    pub fn fields(&self) -> Vec<(String, String)> {
        let body = String::from_utf8_lossy(&self.body);
        body.split("Content-Disposition: form-data; ")
            .skip(1)
            .filter(|part| !part.contains("filename="))
            .filter_map(|part| {
                let name = part.split('"').nth(1)?.to_owned();
                let value = part.split_once("\r\n\r\n")?.1.split("\r\n--").next()?.to_owned();
                Some((name, value))
            })
            .collect()
    }

    /// The uploaded file's bytes.
    pub fn file(&self) -> Option<Vec<u8>> {
        let at = find(&self.body, b"filename=")?;
        let start = at + find(&self.body[at..], b"\r\n\r\n")? + 4;
        let end = start + find(&self.body[start..], b"\r\n--")?;
        Some(self.body[start..end].to_vec())
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A fake HTTP API: request `n` gets `responses[n]` (status, JSON body).
pub struct FakeApi {
    /// `http://127.0.0.1:port`
    pub base: String,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl FakeApi {
    pub async fn start(responses: Vec<(u16, String)>) -> FakeApi {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        tokio::spawn(async move {
            for (status, body) in responses {
                let Ok((mut tcp, _)) = listener.accept().await else { return };
                let request = read_request(&mut tcp).await;
                seen.lock().unwrap().push(request);
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = tcp.write_all(reply.as_bytes()).await;
                let _ = tcp.shutdown().await;
            }
        });
        FakeApi { base, requests }
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

async fn read_request(tcp: &mut tokio::net::TcpStream) -> HttpRequest {
    let mut data = Vec::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = tcp.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(end) = find(&data, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..end]).into_owned();
            let request = HttpRequest { head, body: Vec::new() };
            let length: usize = request.header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
            if data.len() >= end + 4 + length {
                return HttpRequest { body: data[end + 4..end + 4 + length].to_vec(), ..request };
            }
        }
    }
    HttpRequest { head: String::from_utf8_lossy(&data).into_owned(), body: Vec::new() }
}
