# 0022 Meeting language setting, default Auto

## Context

The user's calls mix Vietnamese and English. ElevenLabs was always sent `language_code=en`. Both rows then held Vietnamese speech forced into English words, often garbled, and that also fed the bleed rows of ADR 0021. `docs/research/stt_results.md` tested English only.

What the ElevenLabs docs say:

- `language_code` is optional in both batch and realtime. When it is left out, the language is detected.
- `multilang_mode=true` is supported only on `scribe_v3_live` and follows switches mid-sentence.
- Vietnamese is supported.

Deepgram nova-3 takes `language=multi` for mixed-language audio.

## Decision

- **The setting.** `Settings.language` holds a code. `"auto"` is the default; English, Tiếng Việt and हिन्दी are listed, and a code entered by hand is kept. In the UI it is General → You → Meeting language.
- **Where it goes.** It reaches both live engines (`LiveOptions.language`) and batch (`BatchRequest.language`, `ProcessingConfig.language`).
- **The mapping** is `recap_engine::language_code`:
  - auto or empty → no code;
  - None (older callers) → `en`;
  - anything else → that code.
- **ElevenLabs live:**
  - Auto → `multilang_mode=true` and no `language_code`.
  - A code → `language_code=<code>`.
- **ElevenLabs batch:**
  - Auto → no `language_code`.
  - A code → that code.
- **Deepgram** (live and batch): Auto → `language=multi`; a code → that code.

## Consequences

- Mixed Vietnamese and English calls are transcribed in the language spoken.
- If English-only client calls come out worse on Auto, the user can choose English.
- Keyterms still apply in every mode.
