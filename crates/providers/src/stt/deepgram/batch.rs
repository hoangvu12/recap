//! The after-Stop transcript of one side: `POST /v1/listen?model=nova-3
//! &language=en&smart_format=true&punctuate=true&diarize=…&keyterm=…` with
//! the side as a 16 kHz mono WAV body (transcribe_file.py). Words come from
//! `results.channels[0].alternatives[0].words`.

use recap_engine::{BatchRequest, BatchTranscript, ProviderError, ProviderErrorKind, Word};
use serde::Deserialize;

use super::super::{Speakers, keyterms, wav};
use super::{Inner, http_error};

#[derive(Deserialize)]
struct Response {
    results: Results,
}

#[derive(Deserialize)]
struct Results {
    #[serde(default)]
    channels: Vec<Channel>,
}

#[derive(Deserialize)]
struct Channel {
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Deserialize)]
struct Alternative {
    #[serde(default)]
    words: Vec<ResponseWord>,
}

#[derive(Deserialize)]
struct ResponseWord {
    word: String,
    punctuated_word: Option<String>,
    start: f64,
    end: f64,
    speaker: Option<u32>,
}

pub(super) async fn transcribe(inner: &Inner, request: BatchRequest) -> Result<BatchTranscript, ProviderError> {
    let base = inner.config.api.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}/v1/listen"))
        .map_err(|e| ProviderError::new(ProviderErrorKind::Other, format!("Deepgram: {e}")))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("model", "nova-3")
            .append_pair("language", "en")
            .append_pair("smart_format", "true")
            .append_pair("punctuate", "true")
            .append_pair("diarize", if request.diarize { "true" } else { "false" });
        for term in keyterms(&request.keyterms) {
            query.append_pair("keyterm", &term);
        }
    }
    let client = reqwest::Client::builder()
        .connect_timeout(inner.config.connect_timeout)
        .timeout(inner.config.batch_timeout)
        .build()
        .map_err(|e| ProviderError::new(ProviderErrorKind::Other, e.to_string()))?;
    let network = |e: reqwest::Error| ProviderError::new(ProviderErrorKind::Network, format!("Deepgram: {e}"));
    let response = client
        .post(url)
        .header("authorization", format!("Token {}", inner.key))
        .header("content-type", "audio/wav")
        .body(wav(&request.samples))
        .send()
        .await
        .map_err(network)?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(network)?;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &body));
    }
    let parsed: Response = serde_json::from_str(&body).map_err(|e| {
        ProviderError::new(ProviderErrorKind::Protocol, format!("Deepgram answered something odd: {e}"))
    })?;
    let raw = parsed.results.channels.into_iter().next().and_then(|c| c.alternatives.into_iter().next());
    Ok(BatchTranscript { words: words(raw.map(|a| a.words).unwrap_or_default(), request.diarize) })
}

/// Punctuated words in ms; diarized speakers as `S1`… in order of first
/// appearance (Deepgram's numbers promise no order).
fn words(raw: Vec<ResponseWord>, diarize: bool) -> Vec<Word> {
    let mut speakers = Speakers::default();
    raw.into_iter()
        .map(|w| {
            let speaker = w.speaker.filter(|_| diarize).map(|n| speakers.label(n.to_string()));
            let ms = |s: f64| (s.max(0.0) * 1000.0).round() as u64;
            let (start_ms, end_ms) = (ms(w.start), ms(w.end));
            let text = w.punctuated_word.filter(|t| !t.trim().is_empty()).unwrap_or(w.word);
            Word { text: text.trim().to_owned(), start_ms, end_ms: end_ms.max(start_ms), speaker }
        })
        .collect()
}
