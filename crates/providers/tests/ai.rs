//! The AI provider kinds from the outside: a prompt and a JSON Schema in,
//! validated JSON out. `cli` runs a tiny fake command; the HTTP kinds talk
//! to a local fake server.

mod fake_http;

use std::{path::Path, sync::Arc};

use recap_engine::{Llm, ProviderErrorKind};
use recap_providers::{
    Draft, Job, MemorySecrets, Provider, ProviderSettings, Providers, Registry, Secrets, Slot, TranscriptInput,
};
use serde_json::{Value, json};

fn build(kind: &str, settings: ProviderSettings) -> Arc<dyn Provider> {
    let registry = Registry::builtin();
    (registry.kind(kind).unwrap().build)(&settings)
}

fn llm(kind: &str, settings: ProviderSettings) -> Arc<dyn Llm> {
    build(kind, settings).llm().expect("an AI provider offers an Llm")
}

fn summary_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "next_steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "owner": { "type": ["string", "null"] }, "text": { "type": "string" } },
                    "required": ["owner", "text"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["title", "next_steps"],
        "additionalProperties": false
    })
}

fn good() -> Value {
    json!({ "title": "Launch plan", "next_steps": [{ "owner": "Tom", "text": "Send the deck" }] })
}

// ---------------------------------------------------------------- cli

const FAKE: &str = env!("CARGO_BIN_EXE_fake_ai_cli");

fn cli(command: String, input: TranscriptInput) -> Arc<dyn Llm> {
    llm("cli", ProviderSettings { name: "Fake CLI".into(), command, input, ..Default::default() })
}

/// A `seq` command whose runs print `outs` in turn.
fn seq_dir(outs: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (i, out) in outs.iter().enumerate() {
        std::fs::write(dir.path().join(format!("{}.out", i + 1)), out).unwrap();
    }
    dir
}

fn seq_command(dir: &Path) -> String {
    format!("\"{FAKE}\" seq \"{}\"", dir.display())
}

fn runs(dir: &Path) -> u32 {
    std::fs::read_to_string(dir.join("count")).unwrap().trim().parse().unwrap()
}

#[tokio::test]
async fn a_command_gets_the_task_on_stdin_from_an_empty_folder_without_anthropic_keys() {
    // The app's own environment may hold an API key: the child must not see
    // it, or Claude Code bills the API instead of the user's login.
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-should-not-leak");
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "should-not-leak");
    }
    let llm = cli(format!("\"{FAKE}\" env --json-schema {{schema}} --tools \"\" -p {{prompt}}"), TranscriptInput::Stdin);
    let schema = json!({ "type": "object" });
    let seen = llm.complete_json("Summarise: Tom will send the deck.".into(), schema.clone()).await.unwrap();
    assert_eq!(seen["anthropic_api_key"], false);
    assert_eq!(seen["anthropic_auth_token"], false);
    assert_eq!(seen["cwd_entries"], 0, "runs from an empty folder");
    assert!(seen["stdin"].as_str().unwrap().contains("Tom will send the deck."), "{seen}");
    let args: Vec<&str> = seen["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert_eq!(args[0], "--json-schema");
    assert_eq!(serde_json::from_str::<Value>(args[1]).unwrap(), schema, "{{schema}} is the schema as one argument");
    assert_eq!(args[3], "", "\"\" is an empty argument");
    assert!(args[5].contains("standard input"), "{{prompt}} points at stdin: {}", args[5]);
}

#[tokio::test]
async fn a_command_can_read_the_task_from_a_file() {
    let llm = cli(format!("\"{FAKE}\" env --file {{transcript_file}}"), TranscriptInput::File);
    let seen = llm.complete_json("Summarise: Priya owns the India soft launch.".into(), json!({})).await.unwrap();
    assert!(seen["file"].as_str().unwrap().contains("Priya owns the India soft launch."), "{seen}");
    assert_eq!(seen["stdin"], "");
}

#[tokio::test]
async fn claude_codes_structured_output_is_the_answer() {
    let envelope = json!({
        "type": "result", "subtype": "success", "is_error": false,
        "result": "{\"title\":\"ignored\"}",
        "structured_output": good(),
    });
    let dir = seq_dir(&[&envelope.to_string()]);
    let answer = cli(seq_command(dir.path()), TranscriptInput::Stdin).complete_json("p".into(), summary_schema()).await.unwrap();
    assert_eq!(answer, good());
}

#[tokio::test]
async fn a_plain_text_reply_is_read_as_json_even_in_a_code_fence() {
    let dir = seq_dir(&[&format!("Here you go:\n```json\n{}\n```\n", good())]);
    let answer = cli(seq_command(dir.path()), TranscriptInput::Stdin).complete_json("p".into(), summary_schema()).await.unwrap();
    assert_eq!(answer, good());
}

#[tokio::test]
async fn a_command_gets_one_retry_when_its_json_does_not_match() {
    let dir = seq_dir(&[r#"{"title": "Launch plan"}"#, &good().to_string()]);
    let answer = cli(seq_command(dir.path()), TranscriptInput::Stdin).complete_json("p".into(), summary_schema()).await.unwrap();
    assert_eq!(answer, good());
    assert_eq!(runs(dir.path()), 2);
}

#[tokio::test]
async fn claude_code_errors_say_whether_to_fall_back() {
    let cases = [
        (json!({"type": "result", "is_error": true, "api_error_status": 401, "result": "Invalid API key · Please run /login"}), ProviderErrorKind::Auth),
        (json!({"type": "result", "is_error": true, "api_error_status": 429, "result": "Claude usage limit reached"}), ProviderErrorKind::Quota),
        (json!({"type": "result", "is_error": true, "api_error_status": 529, "result": "Overloaded"}), ProviderErrorKind::Network),
    ];
    for (envelope, kind) in cases {
        let dir = seq_dir(&[&envelope.to_string()]);
        std::fs::write(dir.path().join("1.code"), "1").unwrap();
        let error = cli(seq_command(dir.path()), TranscriptInput::Stdin).complete_json("p".into(), json!({})).await.unwrap_err();
        assert_eq!(error.kind, kind, "{envelope}: {error:?}");
        assert!(error.falls_back());
    }
}

#[tokio::test]
async fn a_failing_command_reports_its_error_output() {
    let dir = seq_dir(&[""]);
    std::fs::write(dir.path().join("1.err"), "boom: something broke").unwrap();
    std::fs::write(dir.path().join("1.code"), "3").unwrap();
    let error = cli(seq_command(dir.path()), TranscriptInput::Stdin).complete_json("p".into(), json!({})).await.unwrap_err();
    assert!(error.message.contains("boom: something broke"), "{error:?}");
}

#[tokio::test]
async fn a_missing_program_falls_back() {
    let error = cli("recap-no-such-program -p".into(), TranscriptInput::Stdin).complete_json("p".into(), json!({})).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Network, "{error:?}");
    assert!(error.message.contains("recap-no-such-program"), "{error:?}");
}

/// The Claude Code preset for real: the user's own login, no API key.
/// `cargo test -p recap-providers --test ai -- --ignored claude_code_for_real`
#[tokio::test]
#[ignore = "runs the installed Claude Code CLI"]
async fn claude_code_for_real() {
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-invalid-must-be-stripped") };
    let registry = Registry::builtin();
    let preset = registry.preset("claude_code").unwrap();
    let llm = llm("cli", ProviderSettings { name: "Claude Code".into(), command: preset.command.into(), ..Default::default() });
    let schema = json!({
        "type": "object",
        "properties": { "launch_day": { "type": "string" }, "owner": { "type": "string" } },
        "required": ["launch_day", "owner"],
        "additionalProperties": false
    });
    let started = std::time::Instant::now();
    let answer = llm
        .complete_json("Tom: we launch on December 1. Priya, you own the India soft launch. Give the launch day and the soft-launch owner.".into(), schema)
        .await
        .unwrap();
    eprintln!("claude -p answered in {:?}: {answer}", started.elapsed());
    assert!(answer["launch_day"].as_str().unwrap().contains('1'), "{answer}");
    assert_eq!(answer["owner"], "Priya");
}

// ---------------------------------------------------------------- openai_compatible

fn chat(content: &str) -> Value {
    json!({ "choices": [{ "index": 0, "message": { "role": "assistant", "content": content }, "finish_reason": "stop" }] })
}

fn openai(base_url: &str) -> Arc<dyn Llm> {
    llm(
        "openai_compatible",
        ProviderSettings {
            name: "OpenRouter".into(),
            key: Some("sk-or-test".into()),
            model: "deepseek/deepseek-chat-v3".into(),
            base_url: base_url.into(),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn openai_compatible_posts_a_chat_completion_and_reads_its_json() {
    let server = fake_http::serve(vec![(200, chat(&good().to_string()))]).await;
    let answer = openai(&format!("{}/api/v1/", server.url)).complete_json("Summarise this".into(), summary_schema()).await.unwrap();
    assert_eq!(answer, good());
    let request = &server.requests()[0];
    assert_eq!((request.method.as_str(), request.path.as_str()), ("POST", "/api/v1/chat/completions"));
    assert_eq!(request.header("authorization"), Some("Bearer sk-or-test"));
    assert_eq!(request.body["model"], "deepseek/deepseek-chat-v3");
    assert_eq!(request.body["response_format"]["type"], "json_object");
    let messages = request.body["messages"].as_array().unwrap();
    assert_eq!(messages[0]["role"], "system");
    let user = messages.last().unwrap()["content"].as_str().unwrap();
    assert!(user.contains("Summarise this") && user.contains("\"next_steps\""), "the prompt carries the schema: {user}");
}

#[tokio::test]
async fn a_local_server_needs_no_key() {
    let server = fake_http::serve(vec![(200, chat("{}"))]).await;
    let llm = llm("openai_compatible", ProviderSettings { name: "Ollama".into(), model: "llama3.1:8b".into(), base_url: server.url.clone(), ..Default::default() });
    llm.complete_json("p".into(), json!({ "type": "object" })).await.unwrap();
    assert_eq!(server.requests()[0].header("authorization"), None);
}

#[tokio::test]
async fn an_invalid_reply_is_retried_once_with_the_error() {
    let server = fake_http::serve(vec![
        (200, chat(r#"{"title": "Launch plan", "next_steps": [{"text": "Send the deck"}]}"#)),
        (200, chat(&good().to_string())),
    ])
    .await;
    let answer = openai(&server.url).complete_json("p".into(), summary_schema()).await.unwrap();
    assert_eq!(answer, good());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let messages = requests[1].body["messages"].as_array().unwrap();
    let n = messages.len();
    assert_eq!(messages[n - 2]["role"], "assistant", "the rejected reply goes back");
    let retry = messages[n - 1]["content"].as_str().unwrap();
    assert!(retry.contains("next_steps[0]") && retry.contains("owner"), "the retry says what was wrong: {retry}");
}

#[tokio::test]
async fn two_invalid_replies_are_a_protocol_error() {
    let server = fake_http::serve(vec![(200, chat("Sorry, I can't help with that.")), (200, chat(r#"{"title": 7, "next_steps": []}"#))]).await;
    let error = openai(&server.url).complete_json("p".into(), summary_schema()).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Protocol, "{error:?}");
    assert!(error.message.contains("title"), "{error:?}");
    assert!(!error.falls_back());
    assert_eq!(server.requests().len(), 2, "one retry, no more");
}

#[tokio::test]
async fn http_errors_map_to_fallback_kinds() {
    let error_body = json!({ "error": { "message": "Nope", "type": "x" } });
    for (status, kind) in [
        (401, ProviderErrorKind::Auth),
        (403, ProviderErrorKind::Auth),
        (402, ProviderErrorKind::Quota),
        (429, ProviderErrorKind::Quota),
        (503, ProviderErrorKind::Network),
        (404, ProviderErrorKind::Other),
    ] {
        let server = fake_http::serve(vec![(status, error_body.clone())]).await;
        let error = openai(&server.url).complete_json("p".into(), json!({})).await.unwrap_err();
        assert_eq!(error.kind, kind, "{status}: {error:?}");
        assert!(error.message.contains("Nope"), "{status}: {error:?}");
    }
}

#[tokio::test]
async fn an_unreachable_server_is_a_network_error() {
    let error = openai("http://127.0.0.1:9").complete_json("p".into(), json!({})).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Network, "{error:?}");
}

// ---------------------------------------------------------------- anthropic

fn message(text: &str) -> Value {
    json!({ "type": "message", "role": "assistant", "stop_reason": "end_turn", "content": [{ "type": "text", "text": text }] })
}

fn anthropic(base_url: &str) -> Arc<dyn Llm> {
    llm(
        "anthropic",
        ProviderSettings {
            name: "Anthropic".into(),
            key: Some("sk-ant-test".into()),
            model: "claude-opus-5-5".into(),
            base_url: base_url.into(),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn anthropic_asks_for_structured_output_held_to_the_schema() {
    let server = fake_http::serve(vec![(200, message(&good().to_string()))]).await;
    let answer = anthropic(&server.url).complete_json("Summarise this".into(), summary_schema()).await.unwrap();
    assert_eq!(answer, good());
    let request = &server.requests()[0];
    assert_eq!(request.path, "/v1/messages");
    assert_eq!(request.header("x-api-key"), Some("sk-ant-test"));
    assert!(request.header("anthropic-version").is_some());
    assert_eq!(request.body["model"], "claude-opus-5-5");
    assert!(request.body["max_tokens"].as_u64().unwrap() >= 4096);
    assert_eq!(request.body["output_config"]["format"], json!({ "type": "json_schema", "schema": summary_schema() }));
    assert_eq!(request.body["messages"][0]["role"], "user");
}

#[tokio::test]
async fn anthropic_errors_map_to_fallback_kinds() {
    let cases = [
        (401, "authentication_error", "invalid x-api-key", ProviderErrorKind::Auth),
        (400, "invalid_request_error", "Your credit balance is too low to access the Anthropic API.", ProviderErrorKind::Quota),
        (429, "rate_limit_error", "Too many requests", ProviderErrorKind::Quota),
        (529, "overloaded_error", "Overloaded", ProviderErrorKind::Network),
    ];
    for (status, kind_name, text, kind) in cases {
        let body = json!({ "type": "error", "error": { "type": kind_name, "message": text } });
        let server = fake_http::serve(vec![(status, body)]).await;
        let error = anthropic(&server.url).complete_json("p".into(), json!({})).await.unwrap_err();
        assert_eq!(error.kind, kind, "{status}: {error:?}");
    }
}

#[tokio::test]
async fn a_cut_off_answer_is_an_error() {
    let mut reply = message("{\"title\": \"Laun");
    reply["stop_reason"] = json!("max_tokens");
    let server = fake_http::serve(vec![(200, reply)]).await;
    let error = anthropic(&server.url).complete_json("p".into(), summary_schema()).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Protocol, "{error:?}");
}

// ---------------------------------------------------------------- the service

fn providers() -> (Providers, Arc<MemorySecrets>) {
    let secrets = Arc::new(MemorySecrets::default());
    (Providers::open(Registry::builtin(), None, secrets.clone()), secrets)
}

#[tokio::test]
async fn the_test_button_makes_a_tiny_real_call() {
    let server = fake_http::serve(vec![(200, chat(r#"{"ok": true}"#))]).await;
    let (providers, _) = providers();
    let outcome = providers
        .test(Draft { key: Some("sk-typed".into()), base_url: Some(server.url.clone()), ..Draft::new("openai") })
        .await;
    assert!(outcome.ok, "{outcome:?}");
    let request = &server.requests()[0];
    assert_eq!(request.header("authorization"), Some("Bearer sk-typed"));
    assert_eq!(request.body["model"], "gpt-5-mini");

    let refused = fake_http::serve(vec![(401, json!({ "error": { "message": "Incorrect API key provided" } }))]).await;
    let outcome = providers
        .test(Draft { key: Some("sk-wrong".into()), base_url: Some(refused.url.clone()), ..Draft::new("openai") })
        .await;
    assert!(!outcome.ok);
    assert!(outcome.error.unwrap().contains("Incorrect API key provided"));
}

#[tokio::test]
async fn the_summary_falls_back_from_a_refused_key_to_the_backup() {
    let main_server = fake_http::serve(vec![(401, json!({ "error": { "message": "bad key" } }))]).await;
    let backup_server = fake_http::serve(vec![(200, message(&good().to_string()))]).await;
    let (providers, secrets) = providers();
    let main = providers.save(Draft { base_url: Some(main_server.url.clone()), ..Draft::new("openrouter") }).unwrap();
    secrets.set(&main, "sk-or-bad").unwrap();
    let backup = providers.save(Draft { base_url: Some(backup_server.url.clone()), key: Some("sk-ant-good".into()), ..Draft::new("anthropic") }).unwrap();
    providers.set_job(Job::Summary, Slot::Main, Some(&main)).unwrap();
    providers.set_job(Job::Summary, Slot::Backup, Some(&backup)).unwrap();
    let switched = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = switched.clone();
    let llm = providers.llm(move |notice| seen.lock().unwrap().push(notice)).unwrap();
    let answer = llm.complete_json("p".into(), summary_schema()).await.unwrap();
    assert_eq!(answer, good());
    let switched = switched.lock().unwrap();
    assert_eq!((switched[0].from.as_str(), switched[0].to.as_str()), ("OpenRouter", "Anthropic"));
    assert_eq!(switched[0].error.kind, ProviderErrorKind::Auth);
}
