//! The JSON discipline every AI kind shares (DESIGN §4.4b): the task goes
//! out with its schema, the reply is read as JSON (code fences and prose
//! around it are tolerated) and checked against the schema. A reply that
//! fails is sent back once with what was wrong; a second failure is a
//! `Protocol` error. Providers that enforce the schema themselves pass the
//! check first time; it costs nothing.

use std::{future::Future, sync::Arc};

use recap_engine::{BoxFuture, Llm, ProviderError, ProviderErrorKind};
use serde_json::{Value, json};

/// Every AI kind's system prompt.
pub const SYSTEM: &str = "You turn a task into JSON. Reply with one JSON value that matches the JSON Schema you are \
given: no prose, no Markdown, no code fences.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}

/// What a kind sends: the system prompt and the turns so far (the task;
/// on a retry also the rejected reply and what was wrong with it).
#[derive(Clone, Debug)]
pub struct Conversation {
    pub system: &'static str,
    pub turns: Vec<Turn>,
}

impl Conversation {
    /// One text, for a command line that takes no conversation.
    pub fn flatten(&self) -> String {
        let mut out = String::from(self.system);
        for turn in &self.turns {
            out.push_str("\n\n");
            if turn.role == Role::Assistant {
                out.push_str("Your previous reply was:\n");
            }
            out.push_str(&turn.text);
        }
        out
    }
}

/// The task as the first user turn: the prompt, then the schema.
fn task(prompt: &str, schema: &Value) -> String {
    format!("{prompt}\n\nReply with only JSON that matches this JSON Schema:\n{schema}")
}

/// Ask with `ask`, check, and ask once more with the error. `ask` answers
/// the reply's text.
pub async fn complete<F, Fut>(name: &str, prompt: String, schema: Value, ask: F) -> Result<Value, ProviderError>
where
    F: Fn(Conversation) -> Fut,
    Fut: Future<Output = Result<String, ProviderError>>,
{
    let mut conversation =
        Conversation { system: SYSTEM, turns: vec![Turn { role: Role::User, text: task(&prompt, &schema) }] };
    let reply = ask(conversation.clone()).await?;
    let error = match check(&reply, &schema) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    conversation.turns.push(Turn { role: Role::Assistant, text: reply });
    conversation.turns.push(Turn {
        role: Role::User,
        text: format!("That reply was rejected: {error}. Reply again with only the corrected JSON."),
    });
    let reply = ask(conversation).await?;
    check(&reply, &schema).map_err(|error| {
        ProviderError::new(
            ProviderErrorKind::Protocol,
            format!("{name} answered JSON that doesn't fit, twice: {error}"),
        )
    })
}

/// The Test button's call: the smallest task that proves the provider
/// answers schema-held JSON.
pub fn probe(llm: Arc<dyn Llm>) -> BoxFuture<'static, Result<(), ProviderError>> {
    let schema = json!({
        "type": "object",
        "properties": { "ok": { "type": "boolean" } },
        "required": ["ok"],
        "additionalProperties": false
    });
    let call = llm.complete_json("Set ok to true.".into(), schema);
    Box::pin(async move { call.await.map(|_| ()) })
}

/// The reply as JSON that fits `schema`, or what is wrong with it.
pub fn check(reply: &str, schema: &Value) -> Result<Value, String> {
    let value = extract(reply)?;
    validate(&value, schema)?;
    Ok(value)
}

/// Read a reply as JSON: as is, inside a code fence, or the outermost
/// `{…}` / `[…]` in prose.
pub fn extract(reply: &str) -> Result<Value, String> {
    let text = reply.trim();
    let first_error = match serde_json::from_str(text) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let body = after.split_once('\n').map_or(after, |(_, rest)| rest);
        if let Some(end) = body.find("```")
            && let Ok(value) = serde_json::from_str(body[..end].trim())
        {
            return Ok(value);
        }
    }
    for (open, close) in [('{', '}'), ('[', ']')] {
        if let (Some(start), Some(end)) = (text.find(open), text.rfind(close))
            && start < end
            && let Ok(value) = serde_json::from_str(&text[start..=end])
        {
            return Ok(value);
        }
    }
    if text.is_empty() {
        return Err("the reply was empty, not JSON".into());
    }
    Err(format!("the reply is not JSON ({first_error})"))
}

/// Check `value` against the JSON Schema subset the app's schemas use:
/// `type` (one or several), `enum`, `const`, `properties`, `required`,
/// `additionalProperties`, `items`, `minItems`, `maxItems`, `anyOf`,
/// `oneOf`. Anything else is accepted. The error names the path
/// (`$.next_steps[0].owner`).
pub fn validate(value: &Value, schema: &Value) -> Result<(), String> {
    at(value, schema, "$")
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn is_type(value: &Value, wanted: &str) -> bool {
    match wanted {
        "number" => value.is_number(),
        "integer" => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        other => type_name(value) == other,
    }
}

fn at(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let schema = match schema {
        Value::Bool(true) => return Ok(()),
        Value::Bool(false) => return Err(format!("{path}: not allowed")),
        Value::Object(schema) => schema,
        _ => return Ok(()),
    };
    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(options)) = schema.get(key)
            && !options.iter().any(|option| at(value, option, path).is_ok())
        {
            return Err(format!("{path}: matches none of the allowed shapes"));
        }
    }
    if let Some(wanted) = schema.get("type") {
        let names: Vec<&str> = match wanted {
            Value::String(name) => vec![name.as_str()],
            Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !names.is_empty() && !names.iter().any(|name| is_type(value, name)) {
            return Err(format!("{path}: expected {}, got {}", names.join(" or "), type_name(value)));
        }
    }
    if let Some(Value::Array(allowed)) = schema.get("enum")
        && !allowed.contains(value)
    {
        return Err(format!("{path}: {value} is not one of {}", Value::Array(allowed.clone())));
    }
    if let Some(expected) = schema.get("const")
        && expected != value
    {
        return Err(format!("{path}: expected {expected}"));
    }
    match value {
        Value::Object(object) => {
            if let Some(Value::Array(required)) = schema.get("required") {
                for name in required.iter().filter_map(Value::as_str) {
                    if !object.contains_key(name) {
                        return Err(format!("{path}.{name}: missing"));
                    }
                }
            }
            let properties = schema.get("properties").and_then(Value::as_object);
            for (name, item) in object {
                let child = format!("{path}.{name}");
                match properties.and_then(|p| p.get(name)) {
                    Some(property) => at(item, property, &child)?,
                    None => match schema.get("additionalProperties") {
                        Some(Value::Bool(false)) => return Err(format!("{child}: not in the schema")),
                        Some(extra) => at(item, extra, &child)?,
                        None => {}
                    },
                }
            }
        }
        Value::Array(items) => {
            let count = items.len() as u64;
            if let Some(min) = schema.get("minItems").and_then(Value::as_u64)
                && count < min
            {
                return Err(format!("{path}: has {count} items, needs at least {min}"));
            }
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
                && count > max
            {
                return Err(format!("{path}: has {count} items, at most {max} allowed"));
            }
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    at(item, item_schema, &format!("{path}[{i}]"))?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_names_the_path_of_what_is_wrong() {
        let schema = json!({
            "type": "object",
            "properties": {
                "speakers": { "type": "object", "additionalProperties": {
                    "type": "object",
                    "properties": { "name": { "type": ["string", "null"] }, "confidence": { "enum": ["high", "low"] } },
                    "required": ["name"]
                }},
                "topics": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
            },
            "required": ["speakers", "topics"],
            "additionalProperties": false
        });
        let ok = json!({ "speakers": { "S1": { "name": null, "confidence": "low" } }, "topics": ["Launch"] });
        assert_eq!(validate(&ok, &schema), Ok(()));
        let cases = [
            (json!({ "speakers": {}, "topics": [] }), "$.topics: has 0 items, needs at least 1"),
            (json!({ "speakers": { "S2": {} }, "topics": ["a"] }), "$.speakers.S2.name: missing"),
            (
                json!({ "speakers": { "S2": { "name": 3 } }, "topics": ["a"] }),
                "$.speakers.S2.name: expected string or null, got integer",
            ),
            (json!({ "speakers": {}, "topics": ["a", 2] }), "$.topics[1]: expected string, got integer"),
            (json!({ "speakers": {}, "topics": ["a"], "extra": 1 }), "$.extra: not in the schema"),
            (
                json!({ "speakers": { "S1": { "name": "Tom", "confidence": "sure" } }, "topics": ["a"] }),
                "$.speakers.S1.confidence: \"sure\" is not one of [\"high\",\"low\"]",
            ),
        ];
        for (value, error) in cases {
            assert_eq!(validate(&value, &schema), Err(error.to_string()), "{value}");
        }
    }

    #[test]
    fn json_is_found_in_fences_and_prose() {
        assert_eq!(extract(" {\"a\": 1} "), Ok(json!({"a": 1})));
        assert_eq!(extract("```json\n{\"a\": 1}\n```"), Ok(json!({"a": 1})));
        assert_eq!(extract("Sure! {\"a\": {\"b\": 2}} Hope that helps."), Ok(json!({"a": {"b": 2}})));
        assert_eq!(extract("[1, 2]"), Ok(json!([1, 2])));
        assert!(extract("no json here").unwrap_err().starts_with("the reply is not JSON"));
        assert_eq!(extract("  "), Err("the reply was empty, not JSON".into()));
    }

    #[test]
    fn a_retry_carries_the_rejected_reply() {
        let conversation = Conversation {
            system: "S",
            turns: vec![
                Turn { role: Role::User, text: "task".into() },
                Turn { role: Role::Assistant, text: "{}".into() },
                Turn { role: Role::User, text: "rejected: x".into() },
            ],
        };
        assert_eq!(conversation.flatten(), "S\n\ntask\n\nYour previous reply was:\n{}\n\nrejected: x");
    }
}
