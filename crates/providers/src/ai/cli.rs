//! A command-line AI (`cli`): Claude Code, Codex, Gemini CLI or a custom
//! command (DESIGN §4.4).
//!
//! The command template is split into arguments like a Windows command
//! line (`"…"` groups, `""` is an empty argument) and run directly, no
//! shell, so a token's text never becomes syntax. In each argument:
//! - `{schema}`: the JSON Schema, compact;
//! - `{transcript_file}`: a file holding the task (prompt, transcript and
//!   schema);
//! - `{prompt}`: a short instruction that points at the task: on stdin
//!   (`TranscriptInput::Stdin`, where the task is written) or in the file.
//!
//! It runs from an empty temp folder, without a console window, and with
//! `ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` removed from its
//! environment: Claude Code would bill the API instead of the user's own
//! login. The reply is stdout; Claude Code's `--output-format json`
//! envelope (`structured_output`, `result`, `is_error`) and Gemini CLI's
//! (`response`) are unwrapped. Then the JSON discipline ([`super::json`]).

use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use recap_engine::{BoxFuture, Capabilities, Llm, ProviderError, ProviderErrorKind};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use super::json as discipline;
use crate::{
    config::TranscriptInput,
    provider::{Provider, ProviderSettings},
    registry::{Family, Kind},
};

pub const KIND: Kind = Kind { id: "cli", family: Family::Ai, capabilities: NONE, build };

const NONE: Capabilities =
    Capabilities { live: false, batch: false, diarize: false, keyterms: false, word_timestamps: false };

/// Never passed to the command.
pub const STRIPPED_ENV: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

/// A one-hour meeting's summary takes Claude Code well under a minute.
const TIMEOUT: Duration = Duration::from_secs(600);

fn build(settings: &ProviderSettings) -> Arc<dyn Provider> {
    Arc::new(Cli(Arc::new(settings.clone())))
}

/// The program a command template runs, as the Providers page shows it
/// (`claude` for `claude -p …`, `fake` for `"C:\x\fake.exe" …`).
pub fn program(command: &str) -> String {
    let first = split(command).into_iter().next().unwrap_or_default();
    let file = first.rsplit(['\\', '/']).next().unwrap_or_default();
    let lower = file.to_ascii_lowercase();
    match [".exe", ".cmd", ".bat", ".com"].iter().find(|ext| lower.ends_with(**ext)) {
        Some(ext) => file[..file.len() - ext.len()].to_string(),
        None => file.to_string(),
    }
}

/// A command line's arguments: whitespace separates, `"` groups (and
/// `""` is an empty argument). Backslashes are literal (Windows paths).
pub fn split(command: &str) -> Vec<String> {
    let mut args = Vec::new();
    let (mut current, mut quoted, mut started) = (String::new(), false, false);
    for c in command.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

#[derive(Clone)]
struct Cli(Arc<ProviderSettings>);

impl Provider for Cli {
    fn llm(&self) -> Option<Arc<dyn Llm>> {
        Some(Arc::new(self.clone()))
    }
    fn test(&self) -> BoxFuture<'static, Result<(), ProviderError>> {
        discipline::probe(Arc::new(self.clone()))
    }
}

impl Llm for Cli {
    fn name(&self) -> &str {
        &self.0.name
    }
    fn complete_json(&self, prompt: String, schema: Value) -> BoxFuture<'static, Result<Value, ProviderError>> {
        let settings = self.0.clone();
        Box::pin(async move {
            let send = |conversation| run(settings.clone(), schema.clone(), conversation);
            discipline::complete(&settings.name, prompt, schema.clone(), send).await
        })
    }
}

fn other(message: String) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Other, message)
}

/// `program` as Windows would find it: on PATH with one of PATHEXT's
/// extensions (so `codex` finds `codex.cmd`). A path or a name with an
/// extension stays as it is.
fn resolve(program: &str) -> PathBuf {
    let path = PathBuf::from(program);
    if !cfg!(windows) || path.components().count() > 1 || path.extension().is_some() {
        return path;
    }
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let dirs = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&dirs) {
        for ext in exts.split(';').filter(|e| !e.is_empty()) {
            let candidate = dir.join(format!("{program}{}", ext.to_ascii_lowercase()));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    path
}

async fn run(
    settings: Arc<ProviderSettings>,
    schema: Value,
    conversation: discipline::Conversation,
) -> Result<String, ProviderError> {
    let name = &settings.name;
    let template = split(&settings.command);
    if template.is_empty() {
        return Err(other(format!("{name} has no command")));
    }
    let folder = tempfile::Builder::new()
        .prefix("recap-ai-")
        .tempdir()
        .map_err(|e| other(format!("Can't make a temp folder for {name}: {e}")))?;
    let work = folder.path().join("work");
    std::fs::create_dir(&work).map_err(|e| other(format!("Can't make a temp folder for {name}: {e}")))?;
    let task = conversation.flatten();
    let task_file = folder.path().join("task.md");
    let file_input = settings.input == TranscriptInput::File;
    if file_input || settings.command.contains("{transcript_file}") {
        tokio::fs::write(&task_file, &task).await.map_err(|e| other(format!("Can't write {name}'s task file: {e}")))?;
    }
    let directive = if file_input {
        format!("Do the task in the file {}. Reply with only JSON.", task_file.display())
    } else {
        "Do the task given on standard input. Reply with only JSON.".to_string()
    };
    let schema_text = schema.to_string();
    let task_path = task_file.display().to_string();
    let args: Vec<String> = template
        .iter()
        .map(|a| a.replace("{schema}", &schema_text).replace("{transcript_file}", &task_path).replace("{prompt}", &directive))
        .collect();

    let program = resolve(&args[0]);
    let mut command = tokio::process::Command::new(&program);
    command
        .args(&args[1..])
        .current_dir(&work)
        .stdin(if file_input { Stdio::null() } else { Stdio::piped() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for var in STRIPPED_ENV {
        command.env_remove(var);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            // Not installed (or not on PATH): let the backup take the job.
            ProviderError::new(ProviderErrorKind::Network, format!("Can't run {}: not found. Is it installed?", args[0]))
        } else {
            other(format!("Can't run {}: {e}", args[0]))
        }
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        // Written beside the wait, so a command that answers before reading
        // all of it can't deadlock us.
        tokio::spawn(async move {
            let _ = stdin.write_all(task.as_bytes()).await;
        });
    }
    let output = tokio::time::timeout(TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| ProviderError::new(ProviderErrorKind::Network, format!("{name} took longer than 10 minutes")))?
        .map_err(|e| other(format!("{name} failed: {e}")))?;
    drop(folder);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    interpret(name, output.status.success(), &stdout, &stderr)
}

/// The reply text in what the command printed, or why it failed.
fn interpret(name: &str, success: bool, stdout: &str, stderr: &str) -> Result<String, ProviderError> {
    if let Ok(Value::Object(envelope)) = serde_json::from_str::<Value>(stdout.trim()) {
        // Claude Code: {"type": "result", "is_error", "result", "structured_output", "api_error_status"}.
        if envelope.get("type").and_then(Value::as_str) == Some("result") && envelope.contains_key("is_error") {
            let result = envelope.get("result").and_then(Value::as_str).unwrap_or_default();
            if envelope["is_error"] == true {
                let status = envelope.get("api_error_status").and_then(Value::as_u64);
                let detail = if result.is_empty() { stderr } else { result };
                return Err(classify(name, status, detail));
            }
            if let Some(structured) = envelope.get("structured_output").filter(|v| !v.is_null()) {
                return Ok(structured.to_string());
            }
            return Ok(result.to_string());
        }
        // Gemini CLI: {"response", "stats"} or {"error": {"message", "code"}}.
        if envelope.contains_key("stats") || envelope.contains_key("error") {
            if let Some(response) = envelope.get("response").and_then(Value::as_str) {
                return Ok(response.to_string());
            }
            if let Some(error) = envelope.get("error").filter(|e| e.is_object()) {
                let status = error.get("code").and_then(Value::as_u64);
                let message = error.get("message").and_then(Value::as_str).unwrap_or_default();
                return Err(classify(name, status, message));
            }
        }
    }
    if !success {
        let detail = if stderr.trim().is_empty() { stdout } else { stderr };
        return Err(classify(name, None, detail));
    }
    Ok(stdout.to_string())
}

/// A command's failure as a provider error: by the HTTP status it reports,
/// else by its words.
fn classify(name: &str, status: Option<u64>, detail: &str) -> ProviderError {
    let lower = detail.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let kind = match status {
        Some(401 | 403) => ProviderErrorKind::Auth,
        Some(402 | 429) => ProviderErrorKind::Quota,
        Some(408 | 500..=599) => ProviderErrorKind::Network,
        _ if has(&["/login", "api key", "not logged in", "unauthorized", "authenticat"]) => ProviderErrorKind::Auth,
        _ if has(&["usage limit", "rate limit", "quota", "credit balance"]) => ProviderErrorKind::Quota,
        _ if has(&["overloaded", "econnrefused", "network", "timed out"]) => ProviderErrorKind::Network,
        _ => ProviderErrorKind::Other,
    };
    ProviderError::new(kind, format!("{name}: {}", tail(detail)))
}

/// The end of a long output, where the error usually is.
fn tail(text: &str) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= 500 { text.to_string() } else { format!("…{}", text.chars().skip(count - 500).collect::<String>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_split_like_a_windows_command_line() {
        assert_eq!(
            split(r#"claude -p --json-schema {schema} --tools "" --no-session-persistence"#),
            ["claude", "-p", "--json-schema", "{schema}", "--tools", "", "--no-session-persistence"]
        );
        assert_eq!(split(r#""C:\Program Files\x\ai.exe" run  "a b""#), [r"C:\Program Files\x\ai.exe", "run", "a b"]);
        assert!(split("   ").is_empty());
    }

    #[test]
    fn the_program_is_named_without_its_folder_or_extension() {
        assert_eq!(program("claude -p --output-format json"), "claude");
        assert_eq!(program(r#""C:\Tools\My AI\gemini.CMD" -p {prompt}"#), "gemini");
        assert_eq!(program(""), "");
    }
}
