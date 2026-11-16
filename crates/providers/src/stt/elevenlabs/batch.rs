//! The after-Stop transcript of one side: `POST /v1/speech-to-text` with
//! `model_id=scribe_v3, language_code=en, no_verbatim=true,
//! timestamps_granularity=word`, `diarize` as asked and the keyterms
//! (reference/el_batch.sh). The side goes up as a 16 kHz mono WAV.

use recap_engine::{BatchRequest, BatchTranscript, ProviderError, ProviderErrorKind, SAMPLE_RATE, Word};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;

use super::{Inner, http_error, keyterms};

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    words: Vec<ResponseWord>,
}

#[derive(Deserialize)]
struct ResponseWord {
    text: String,
    start: Option<f64>,
    end: Option<f64>,
    #[serde(rename = "type", default)]
    kind: String,
    speaker_id: Option<String>,
}

pub(super) async fn transcribe(inner: &Inner, request: BatchRequest) -> Result<BatchTranscript, ProviderError> {
    let mut form = Form::new()
        .text("model_id", "scribe_v3")
        .text("language_code", "en")
        .text("diarize", if request.diarize { "true" } else { "false" })
        .text("no_verbatim", "true")
        .text("timestamps_granularity", "word");
    for term in keyterms(&request.keyterms) {
        form = form.text("keyterms", term);
    }
    let file = Part::bytes(wav(&request.samples))
        .file_name("side.wav")
        .mime_str("audio/wav")
        .expect("a valid mime type");
    form = form.part("file", file);

    let url = format!("{}/v1/speech-to-text", inner.config.api.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .connect_timeout(inner.config.connect_timeout)
        .timeout(inner.config.batch_timeout)
        .build()
        .map_err(|e| ProviderError::new(ProviderErrorKind::Other, e.to_string()))?;
    let response = client
        .post(url)
        .header("xi-api-key", &inner.key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| ProviderError::new(ProviderErrorKind::Network, format!("ElevenLabs: {e}")))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| ProviderError::new(ProviderErrorKind::Network, format!("ElevenLabs: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &body));
    }
    let parsed: Response = serde_json::from_str(&body)
        .map_err(|e| ProviderError::new(ProviderErrorKind::Protocol, format!("ElevenLabs answered something odd: {e}")))?;
    Ok(BatchTranscript { words: words(parsed.words, request.diarize) })
}

/// Words only (no spacing or audio events), in ms; diarized speakers as
/// `S1`… in order of first appearance.
fn words(raw: Vec<ResponseWord>, diarize: bool) -> Vec<Word> {
    let mut speakers: Vec<String> = Vec::new();
    raw.into_iter()
        .filter(|w| w.kind.is_empty() || w.kind == "word")
        .map(|w| {
            let speaker = w.speaker_id.filter(|_| diarize).map(|id| {
                let n = match speakers.iter().position(|s| *s == id) {
                    Some(i) => i + 1,
                    None => {
                        speakers.push(id);
                        speakers.len()
                    }
                };
                format!("S{n}")
            });
            let ms = |s: Option<f64>| (s.unwrap_or(0.0).max(0.0) * 1000.0).round() as u64;
            let (start_ms, end_ms) = (ms(w.start), ms(w.end));
            Word { text: w.text.trim().to_owned(), start_ms, end_ms: end_ms.max(start_ms), speaker }
        })
        .collect()
}

/// 16 kHz mono 16-bit PCM as a WAV file.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
