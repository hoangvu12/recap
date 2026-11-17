//! Any OpenAI-compatible chat API (`openai_compatible`): base URL + key +
//! model. OpenAI, OpenRouter, Groq, DeepSeek, Mistral, Gemini and Ollama
//! are presets of it.
//!
//! `POST {base_url}/chat/completions` with JSON mode
//! (`response_format: json_object`), which all of them accept; strict
//! `json_schema` mode is not (DeepSeek and some OpenRouter models refuse
//! it). So the schema travels in the prompt and the reply is validated and
//! retried once with the error ([`super::json`], ADR 0008). No key: no
//! `Authorization` header (Ollama).

use std::sync::Arc;

use recap_engine::{BoxFuture, Capabilities, Llm, ProviderError, ProviderErrorKind};
use serde_json::{Value, json};

use super::{http, json as discipline};
use crate::{
    provider::{Provider, ProviderSettings},
    registry::{Family, Kind},
};

pub const KIND: Kind = Kind {
    id: "openai_compatible",
    family: Family::Ai,
    capabilities: Capabilities { live: false, batch: false, diarize: false, keyterms: false, word_timestamps: false },
    build,
};

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(OpenAiCompatible(Arc::new(settings.clone())))
}

#[derive(Clone)]
struct OpenAiCompatible(Arc<ProviderSettings>);

impl Provider for OpenAiCompatible {
    fn llm(&self) -> Option<Arc<dyn Llm>> {
        Some(Arc::new(self.clone()))
    }
    fn test(&self) -> BoxFuture<'static, Result<(), ProviderError>> {
        discipline::probe(Arc::new(self.clone()))
    }
}

impl Llm for OpenAiCompatible {
    fn name(&self) -> &str {
        &self.0.name
    }
    fn complete_json(&self, prompt: String, schema: Value) -> BoxFuture<'static, Result<Value, ProviderError>> {
        let settings = self.0.clone();
        Box::pin(async move {
            discipline::complete(&settings.name, prompt, schema, |conversation| ask(settings.clone(), conversation))
                .await
        })
    }
}

async fn ask(settings: Arc<ProviderSettings>, conversation: discipline::Conversation) -> Result<String, ProviderError> {
    let mut messages = vec![json!({ "role": "system", "content": conversation.system })];
    messages.extend(conversation.turns.iter().map(|t| json!({ "role": t.role.as_str(), "content": t.text })));
    let body = json!({
        "model": settings.model,
        "messages": messages,
        "response_format": { "type": "json_object" },
    });
    let mut request = http::client().post(http::join(&settings.base_url, "chat/completions")).json(&body);
    if let Some(key) = settings.key.as_deref().filter(|k| !k.is_empty()) {
        request = request.bearer_auth(key);
    }
    let answer = http::post(&settings.name, request).await?;
    answer.pointer("/choices/0/message/content").and_then(Value::as_str).map(String::from).ok_or_else(|| {
        ProviderError::new(ProviderErrorKind::Protocol, format!("{} sent no answer: {}", settings.name, clip(&answer)))
    })
}

fn clip(value: &Value) -> String {
    value.to_string().chars().take(300).collect()
}
