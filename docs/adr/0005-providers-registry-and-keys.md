# 5. Providers: a registry of kinds and presets, keys in Credential Manager, fallback in the engine

Status: accepted (ticket #5)

## Context

Three jobs use outside providers (DESIGN §4.4b): Live captions (live STT),
Transcript (batch STT with speakers) and Summary and names (an AI). Each job
has a main provider and an optional backup. Right after this ticket, #6
(ElevenLabs), #10 (the AI providers) and #16 (the other Settings pages) are
built in parallel, so adding an adapter must not touch anyone else's code.

## Decision

- **`crates/providers` (recap-providers) holds the registry, the jobs, the
  keys and the `Providers` service.** It has no zgx. The engine keeps the
  traits (`LiveStt`, `BatchStt`, `Llm`, `ProviderError`).
- **A `Kind` is an adapter:** its id, family (STT or AI), capabilities
  (live, batch, diarize, keyterms, word_timestamps) and a `build(&ProviderSettings)`
  that returns a `dyn Provider` (`live()`, `batch()`, `llm()`, `test()`).
  **A `Preset` is a row in the Add provider list:** name, logo, group, which
  fields the form shows, and default model, base URL or command. Several presets
  can share a kind (OpenAI, Groq and DeepSeek all use `openai_compatible`).
  Each speech engine is one file in `stt/`, with a `KIND` and a `PRESET`. Each
  AI kind is one file in `ai/`, and all AI presets are in `ai/presets.rs`.
  `Registry::builtin()` lists them all. Until a ticket builds its adapter, the
  kind returns `NotBuilt`.
- **A job takes a provider only if it has the capabilities the job needs**
  (`Job::needs`). Live needs `live`; Transcript needs `batch`, `diarize` and
  `word_timestamps`; Summary needs an AI. The UI filters its pickers on the
  same published data, and `set_job` checks the rule again.
- **Keys go only to Windows Credential Manager** (`keyring`, service
  "Recap", user = the provider id), behind a `Secrets` trait (`MemorySecrets`
  in tests). `providers.json` in the data folder holds the added AI providers
  and the job choices, never a key. The UI only ever gets the masked form
  (`sk_••••3f9a`).
- **Fallback is engine logic** (`recap_engine::Fallback`). It wraps a main
  and a backup of the same trait. On a quota, auth or network error
  (`ProviderError::falls_back`), it switches once and reports a
  `FallbackNotice`. Live STT keeps the audio since the main's last final (at
  most 30 s) and replays it to the backup, so no words are lost at the
  switch. Other errors stand.
- **The Test button runs `Provider::test()`** on the engine runtime, using
  the form's draft (the typed key, or the stored one). The outcome comes back
  as shared state (`provider_test`), not a promise, so Luau tests can mock it.

## Consequences

- To add a provider, add one file and one registry line (see
  `notes/LEARNINGS.md`). To add a Settings page, add one entry in
  `ui/screens/settings/pages.luau` and, if it needs Rust, a module like
  `crates/app/src/settings.rs`.
- A provider id is its preset id (`openai`, `openai-2`, …) or a slug of
  the name for custom entries. Renaming a provider keeps its id, and so its
  key.
- A key that only exists in Credential Manager is lost if Windows'
  credential store is reset. That is acceptable, because the user can
  re-enter it.
