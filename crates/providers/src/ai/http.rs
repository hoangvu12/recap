//! What the HTTP kinds share: one client, and HTTP failures turned into
//! the error kinds that decide fallback (quota/auth/network switch to the
//! backup).

use std::{sync::OnceLock, time::Duration};

use recap_engine::{ProviderError, ProviderErrorKind};
use serde_json::Value;

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            // A one-hour meeting's summary is one call: give it room.
            .timeout(Duration::from_secs(600))
            .build()
            .expect("an HTTP client")
    })
}

/// `base` + `path`, without a doubled slash.
pub fn join(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path.trim_start_matches('/'))
}

/// Send `request` and read its JSON answer; a failure says `name`.
pub async fn post(name: &str, request: reqwest::RequestBuilder) -> Result<Value, ProviderError> {
    let response = request.send().await.map_err(|error| {
        let kind = if error.is_builder() { ProviderErrorKind::Other } else { ProviderErrorKind::Network };
        ProviderError::new(kind, format!("Can't reach {name}: {}", error.without_url()))
    })?;
    let status = response.status().as_u16();
    let text = response.text().await.map_err(|error| {
        ProviderError::new(ProviderErrorKind::Network, format!("{name} hung up: {}", error.without_url()))
    })?;
    if !(200..300).contains(&status) {
        return Err(status_error(name, status, &text));
    }
    serde_json::from_str(&text).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::Protocol,
            format!("{name} answered something that isn't JSON: {}", clip(&text)),
        )
    })
}

/// The error message of an error body (OpenAI and Anthropic both nest it
/// under `error.message`), else the body itself.
fn message(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let found = parsed.as_ref().and_then(|v| {
        v.pointer("/error/message").or_else(|| v.get("message")).or_else(|| v.get("error")).and_then(Value::as_str)
    });
    clip(found.unwrap_or(body))
}

fn clip(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(300) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

/// HTTP `status` as a provider error: 401/403 auth; 402, 429 and "credit"
/// or "quota" in a 400 (Anthropic's low-balance answer) quota; 408 and 5xx
/// (529 overloaded) network; else other.
pub fn status_error(name: &str, status: u16, body: &str) -> ProviderError {
    let message = message(body);
    let lower = message.to_lowercase();
    let kind = match status {
        401 | 403 => ProviderErrorKind::Auth,
        402 | 429 => ProviderErrorKind::Quota,
        400 if ["credit", "quota", "billing"].iter().any(|w| lower.contains(w)) => ProviderErrorKind::Quota,
        408 | 500..=599 => ProviderErrorKind::Network,
        _ => ProviderErrorKind::Other,
    };
    let text = match kind {
        ProviderErrorKind::Auth => format!("{name} refused the key ({status}): {message}"),
        ProviderErrorKind::Quota => format!("{name} is out of credit or over its limit ({status}): {message}"),
        ProviderErrorKind::Network => format!("{name} is unavailable ({status}): {message}"),
        _ => format!("{name} answered {status}: {message}"),
    };
    ProviderError::new(kind, text)
}
