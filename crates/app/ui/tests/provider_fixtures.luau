--!nonstrict
-- The providers as the host publishes them (crates/providers:
-- Providers::view), screens/8b's: ElevenLabs and Deepgram, Claude Code,
-- OpenRouter and Ollama; plus "Whisper", a speech engine that can only
-- transcribe after Stop, without speakers.
local M = {}

local ALL = { live = true, batch = true, diarize = true, keyterms = true, word_timestamps = true }
local API = { key = true, model = true, base_url = true, command = false, name = false }
local KEY = { key = true, model = false, base_url = false, command = false, name = false }
local CLI = { key = false, model = false, base_url = false, command = true, name = false }
local LOCAL = { key = false, model = true, base_url = true, command = false, name = false }

local function row(t)
	t.kind = t.kind or "openai_compatible"
	t.family = t.family or "ai"
	t.capabilities = t.capabilities
		or { live = false, batch = false, diarize = false, keyterms = false, word_timestamps = false }
	t.key_hint = t.key_hint or ""
	t.key_mask = t.key_mask or ""
	t.trailing = t.trailing or t.key_mask
	t.model = t.model or ""
	t.base_url = t.base_url or ""
	t.command = t.command or ""
	t.input = t.input or "stdin"
	t.subtitle = t.subtitle or t.model
	return t
end

local function preset(id, name, logo, group, kind, fields, detail, extra)
	local p = {
		id = id,
		name = name,
		logo = logo,
		group = group,
		kind = kind,
		fields = fields,
		base_url = "",
		model = "",
		command = "",
		detail = detail or "",
		key_hint = "",
	}
	for k, v in extra or {} do
		p[k] = v
	end
	return p
end

function M.view()
	return {
		jobs = {
			{ id = "live", label = "Live captions", family = "stt", needs = { "live" }, main = "elevenlabs", backup = "deepgram" },
			{
				id = "transcript",
				label = "Transcript",
				family = "stt",
				needs = { "batch", "diarize", "word_timestamps" },
				main = "elevenlabs",
				backup = "deepgram",
			},
			{ id = "summary", label = "Summary and names", family = "ai", needs = {}, main = "claude_code", backup = "openrouter" },
		},
		stt = {
			row {
				id = "elevenlabs",
				preset = "elevenlabs",
				kind = "elevenlabs",
				family = "stt",
				name = "ElevenLabs",
				logo = "elevenlabs",
				capabilities = ALL,
				fields = KEY,
				key_mask = "sk_••••3f9a",
				subtitle = "",
			},
			row {
				id = "deepgram",
				preset = "deepgram",
				kind = "deepgram",
				family = "stt",
				name = "Deepgram",
				logo = "deepgram",
				capabilities = ALL,
				fields = KEY,
				key_mask = "••••a21c",
				subtitle = "",
			},
			row {
				id = "whisper",
				preset = "whisper",
				kind = "whisper",
				family = "stt",
				name = "Whisper",
				logo = "openai",
				capabilities = { live = false, batch = true, diarize = false, keyterms = false, word_timestamps = true },
				fields = KEY,
				subtitle = "",
			},
		},
		ai = {
			row {
				id = "claude_code",
				preset = "claude_code",
				kind = "cli",
				name = "Claude Code",
				logo = "claude",
				fields = CLI,
				command = "claude -p --output-format json",
				subtitle = "claude",
				tested = { ok = true, latency_ms = 5200 },
			},
			row {
				id = "openrouter",
				preset = "openrouter",
				name = "OpenRouter",
				logo = "openrouter",
				fields = API,
				model = "deepseek-chat-v3",
				base_url = "https://openrouter.ai/api/v1",
				key_mask = "sk-or-••••91be",
			},
			row {
				id = "ollama",
				preset = "ollama",
				name = "Ollama",
				logo = "ollama",
				fields = LOCAL,
				model = "llama3.1:8b",
				base_url = "http://localhost:11434/v1",
				trailing = "localhost:11434",
			},
		},
		presets = {
			preset("anthropic", "Anthropic", "anthropic", "api", "anthropic", API, "", { model = "claude-opus-5-5" }),
			preset("openai", "OpenAI", "openai", "api", "openai_compatible", API),
			preset("openrouter", "OpenRouter", "openrouter", "api", "openai_compatible", API, "", {
				base_url = "https://openrouter.ai/api/v1",
				model = "deepseek/deepseek-chat-v3",
				key_hint = "sk-or-…",
			}),
			preset("groq", "Groq", "groq", "api", "openai_compatible", API),
			preset("deepseek", "DeepSeek", "deepseek", "api", "openai_compatible", API),
			preset("mistral", "Mistral", "mistral", "api", "openai_compatible", API),
			preset("gemini", "Gemini", "gemini", "api", "openai_compatible", API),
			preset("ollama", "Ollama", "ollama", "local", "openai_compatible", LOCAL, "localhost:11434"),
			preset("claude_code", "Claude Code", "claude", "cli", "cli", CLI, "claude"),
			preset("codex", "Codex", "codex", "cli", "cli", CLI, "codex"),
			preset("gemini_cli", "Gemini CLI", "gemini", "cli", "cli", CLI, "gemini"),
			preset("openai_compatible", "OpenAI-compatible endpoint", "compatible", "custom", "openai_compatible", {
				key = true,
				model = true,
				base_url = true,
				command = false,
				name = true,
			}),
			preset("custom_command", "Custom command", "command", "custom", "cli", {
				key = false,
				model = false,
				base_url = false,
				command = true,
				name = true,
			}),
		},
	}
end

-- Settings open on the Providers page.
function M.settings(app, view)
	app:set_state("providers", view or M.view())
	app:set_state("settings_page", "providers")
	app:set_state("window.settings", true)
	app:frames(2)
	return app:window("settings")
end

return M
