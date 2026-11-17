//! Anthropic's Messages API, native (`anthropic`): key + model, the reply
//! held to the schema with structured outputs
//! (`output_config.format = {type: json_schema, schema}`). It is still
//! validated like every kind's ([`super::json`]).

use std::sync::Arc;

use recap_engine::{BoxFuture, Capabilities, Llm, ProviderError, ProviderErrorKind};
use serde_json::{Value, json};

use super::{http, json as discipline};
use crate::{
    provider::{Provider, ProviderSettings},
    registry::{Family, Kind},
};

pub const KIND: Kind = Kind {
    id: "anthropic",
    family: Family::Ai,
    capabilities: Capabilities { live: false, batch: false, diarize: false, keyterms: false, word_timestamps: false },
    build,
};

const VERSION: &str = "2026-10-21";
/// A one-hour meeting's summary fits easily; non-streaming stays under
/// the HTTP timeout.
const MAX_TOKENS: u32 = 16_000;

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(Anthropic(Arc::new(settings.clone())))
}

#[derive(Clone)]
struct Anthropic(Arc<ProviderSettings>);

impl Provider for Anthropic {
    fn llm(&self) -> Option<Arc<dyn Llm>> {
        Some(Arc::new(self.clone()))
    }
    fn test(&self) -> BoxFuture<'static, Result<(), ProviderError>> {
        discipline::probe(Arc::new(self.clone()))
    }
}

impl Llm for Anthropic {
    fn name(&self) -> &str {
        &self.0.name
    }
    fn complete_json(&self, prompt: String, schema: Value) -> BoxFuture<'static, Result<Value, ProviderError>> {
        let settings = self.0.clone();
        Box::pin(async move {
            let send = |conversation| ask(settings.clone(), schema.clone(), conversation);
            discipline::complete(&settings.name, prompt, schema.clone(), send).await
        })
    }
}

/// `https://api.anthropic.com` or one ending in `/v1`: the messages URL.
fn url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let base = if base.is_empty() { "https://api.anthropic.com" } else { base };
    if base.ends_with("/v1") { http::join(base, "messages") } else { http::join(base, "v1/messages") }
}

async fn ask(
    settings: Arc<ProviderSettings>,
    schema: Value,
    conversation: discipline::Conversation,
) -> Result<String, ProviderError> {
    let messages: Vec<Value> =
        conversation.turns.iter().map(|t| json!({ "role": t.role.as_str(), "content": t.text })).collect();
    let body = json!({
        "model": settings.model,
        "max_tokens": MAX_TOKENS,
        "system": conversation.system,
        "messages": messages,
        "output_config": { "format": { "type": "json_schema", "schema": schema } },
    });
    let request = http::client()
        .post(url(&settings.base_url))
        .header("x-api-key", settings.key.as_deref().unwrap_or_default())
        .header("anthropic-version", VERSION)
        .json(&body);
    let answer = http::post(&settings.name, request).await?;
    let name = &settings.name;
    match answer.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => {
            return Err(ProviderError::new(ProviderErrorKind::Other, format!("{name} declined to answer")));
        }
        Some("max_tokens") => {
            return Err(ProviderError::new(
                ProviderErrorKind::Protocol,
                format!("{name}'s answer was cut off (too long)"),
            ));
        }
        _ => {}
    }
    let text: String = answer
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    if text.is_empty() {
        return Err(ProviderError::new(ProviderErrorKind::Protocol, format!("{name} sent no answer")));
    }
    Ok(text)
}
