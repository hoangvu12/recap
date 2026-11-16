# 0014 Deepgram adapter, the backup speech engine

## Context

Deepgram Nova-3 is the default backup for Live captions and Transcript
(ticket #14, DESIGN §4.2). There is no Deepgram key, so nothing could be
recorded from the real API. The overlay's "Switch to Deepgram" and the live
status (who captions now, errors told at once) belong to #8 (ADR 0012). Its
`switch_live` action builds the provider from the registry, so it needed
this adapter. Before it, Deepgram was `NotBuilt`.

## Decision

- The adapter has the same shape as ElevenLabs (ADR 0007):
  `recap_providers::deepgram::{Deepgram, Config}`, one file plus `live` and
  `batch` submodules. The two adapters share `stt::{keyterms, wav, Speakers}`.
- **Live:**
  - The `/v1/listen` websocket uses the earlier Python app's settings
    (`live_captions.py`): `nova-3`, `linear16` 16 kHz mono,
    `interim_results`, `smart_format`, `endpointing=300`,
    `utterance_end_ms=1000` and `keyterm`s. Auth is `Authorization: Token`.
  - Audio goes as binary frames. `KeepAlive` is sent every 4 s without
    audio, because Deepgram closes after 10 s. At the end, `CloseStream`,
    then wait for `Metadata` or the close (at most `flush`).
  - Captions:
    - Interim results are partials, shown after the utterance's finals so far.
    - Finals join until `speech_final`, `UtteranceEnd` or about 220
      characters, then go out as one final.
    - Times come from word times, mapped through per-frame anchors onto
      the side's timeline.
  - Reconnect: a drop, `1011 NET-…` or a 5xx upgrade reconnects with
    backoff and replays the audio since the last final. It gives up with
    Network after `retries` in a row.
  - Errors:
    - A refused upgrade is read from `{err_code, err_msg}` (or `dg-error`).
      401/403 are Auth; 402 `ASR_PAYMENT_REQUIRED` and 429 are Quota.
    - `1008`/`1009` closes are Protocol, with no retry.
    - Error text about credit or keys is Quota or Auth.
- **Batch:** `POST /v1/listen` with the side's 16 kHz WAV as the body, and
  `smart_format`, `punctuate`, `diarize` and `keyterm`s (`transcribe_file.py`).
  Punctuated words come back in ms. Deepgram's speaker numbers become `S1..`
  in order of first appearance.
- **Test button:** open a stream, send no audio, then `CloseStream`. It costs nothing.
- **Fixtures:** `deepgram_live.jsonl` / `deepgram_batch.json` are written by
  `tools/gen_deepgram_fixtures.py`. They use Deepgram's documented message
  shapes, filled with the test meeting's word timings from the recorded
  ElevenLabs answer. Speakers are numbered out of order on purpose.
  `tests/deepgram_real.rs` (ignored, `DEEPGRAM_API_KEY`) checks the real API.

## Consequences

- The adapter has not run against the real service. Run `deepgram_real`
  once a key exists, and record real fixtures if the shapes differ.
- With both keys saved, the default jobs fall back from ElevenLabs to
  Deepgram live and after Stop (`tests/deepgram_fallback.rs`, each adapter
  against its fake). Without a Deepgram key the backup is left out, as before.
