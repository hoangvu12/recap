# 0006: AI providers and their JSON discipline

## Context

Summary and names need JSON that fits a schema from any of three AI kinds (ticket #10): `cli`, `openai_compatible` and `anthropic`. Only some providers can enforce a schema: Claude Code's `--json-schema` and Anthropic's structured outputs can. OpenAI-compatible services differ. DeepSeek and some OpenRouter models refuse strict `json_schema` mode, but they all accept JSON mode. A command line may print prose.

## Decision

- **One discipline for every kind** (`ai/json.rs`). The schema goes with the task in the prompt. The reply is read as JSON, even inside a code fence or surrounded by prose. It is then checked with a small built-in validator covering the subset of JSON Schema our schemas use. If the check fails, the conversation is sent once more with the rejected reply and the error, which includes the path, for example `$.next_steps[0].owner: missing`. A second failure is a `Protocol` error, which doesn't fall back. Providers that enforce the schema pass on the first try, so this costs them nothing.
- **openai_compatible** sends `response_format: {type: "json_object"}` and never `json_schema`, so that every preset works.
- **anthropic** uses `output_config.format = json_schema`, `anthropic-version: 2026-10-21` and `max_tokens` 16000, without streaming. `stop_reason: refusal` is an `Other` error and `max_tokens` is a `Protocol` error.
- **cli**:
  - The command is split like a Windows command line (`""` is an empty argument) and run without a shell.
  - It runs from an empty temp folder, with no console window and a 10-minute timeout.
  - `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` are removed for **every** command, not only Claude Code, so a CLI never bills the API by accident.
  - The task (prompt, transcript and schema) goes on stdin, or into the file named by `{transcript_file}`.
  - `{prompt}` becomes a short instruction that points at the task. The task can't be passed as an argument, because a meeting overflows Windows' 32K command line.
  - A bare program name is resolved through PATH and PATHEXT, so `codex` finds `codex.cmd`.
  - The Claude Code and Gemini CLI JSON output wrappers are unwrapped.
- **Error kinds decide fallback:**
  - 401 and 403 are `Auth`.
  - 402, 429, and a 400 that mentions credit, quota or billing (Anthropic's low-balance answer) are `Quota`.
  - Connection failures, 408 and 5xx (529 included) are `Network`.
  - A CLI that isn't installed is `Network`, so the backup takes the job.
  - CLI errors are classified by the wrapper's `api_error_status`, or else by their words.
- **Test** runs a real, tiny `complete_json` call (`{ok: boolean}`) through the same path. Its latency is what the button shows.

## Consequences

- The validator covers only the subset of JSON Schema we use: `type`, `enum`, `const`, `properties`, `required`, `additionalProperties`, `items`, `minItems`/`maxItems`, `anyOf`/`oneOf`. Any other keyword is accepted without a check.
- Anthropic's structured outputs need schemas that it supports: objects with `additionalProperties: false` and `required`. Pipeline schemas should be written that way.
- A custom command that relies on `ANTHROPIC_API_KEY` doesn't get it. Use the Anthropic provider instead.
- With file input, Claude Code (`--tools ""`) can't read the file. Claude Code uses stdin.
