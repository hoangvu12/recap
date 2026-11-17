//! The AI presets of the Add provider list (screens/8c), in its order. Data
//! only: a preset names its kind and fills the form's defaults. Add a
//! service here; no code or UI work.

use crate::registry::{Fields, Group, Preset};

const API: Fields = Fields { key: true, model: true, base_url: true, command: false, name: false };
const LOCAL: Fields = Fields { key: false, model: true, base_url: true, command: false, name: false };
const CLI: Fields = Fields { key: false, model: false, base_url: false, command: true, name: false };

const fn api(
    id: &'static str,
    name: &'static str,
    logo: &'static str,
    base_url: &'static str,
    model: &'static str,
    key_hint: &'static str,
) -> Preset {
    Preset { fields: API, base_url, model, key_hint, ..Preset::new(id, name, logo, Group::Api, "openai_compatible") }
}

const fn cli(id: &'static str, name: &'static str, logo: &'static str, command: &'static str) -> Preset {
    let program = match id.as_bytes() {
        b"claude_code" => "claude",
        b"codex" => "codex",
        _ => "gemini",
    };
    Preset { fields: CLI, command, detail: program, ..Preset::new(id, name, logo, Group::Cli, "cli") }
}

pub const ALL: [Preset; 13] = [
    Preset {
        fields: API,
        base_url: "https://api.anthropic.com",
        model: "claude-opus-5-5",
        key_hint: "sk-ant-…",
        ..Preset::new("anthropic", "Anthropic", "anthropic", Group::Api, "anthropic")
    },
    api("openai", "OpenAI", "openai", "https://api.openai.com/v1", "gpt-5-mini", "sk-…"),
    api(
        "openrouter",
        "OpenRouter",
        "openrouter",
        "https://openrouter.ai/api/v1",
        "deepseek/deepseek-chat-v3",
        "sk-or-…",
    ),
    api("groq", "Groq", "groq", "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", "gsk_…"),
    api("deepseek", "DeepSeek", "deepseek", "https://api.deepseek.com/v1", "deepseek-chat", "sk-…"),
    api("mistral", "Mistral", "mistral", "https://api.mistral.ai/v1", "mistral-large-latest", ""),
    api(
        "gemini",
        "Gemini",
        "gemini",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "gemini-2.5-flash",
        "AIza…",
    ),
    Preset {
        fields: LOCAL,
        base_url: "http://localhost:11434/v1",
        model: "llama3.1:8b",
        detail: "localhost:11434",
        ..Preset::new("ollama", "Ollama", "ollama", Group::Local, "openai_compatible")
    },
    cli(
        "claude_code",
        "Claude Code",
        "claude",
        "claude -p --output-format json --json-schema {schema} --tools \"\" --no-session-persistence",
    ),
    cli("codex", "Codex", "codex", "codex exec --skip-git-repo-check -"),
    cli("gemini_cli", "Gemini CLI", "gemini", "gemini -p {prompt} --output-format json"),
    Preset {
        fields: Fields { name: true, ..API },
        ..Preset::new(
            "openai_compatible",
            "OpenAI-compatible endpoint",
            "compatible",
            Group::Custom,
            "openai_compatible",
        )
    },
    Preset {
        fields: Fields { name: true, ..CLI },
        ..Preset::new("custom_command", "Custom command", "command", Group::Custom, "cli")
    },
];
