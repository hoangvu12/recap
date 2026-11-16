# 0007 ElevenLabs adapter: timed commits, replay on reconnect, native TLS

## Context

Ticket #6 adds live captions over ElevenLabs' realtime websocket, and the
after-Stop transcript (`scribe_v3`) for the processing ticket. Probing the
real API on the test meeting showed:

- `partial_transcript` and `committed_transcript` carry text only. With
  `include_timestamps=true`, every commit is followed about 30 ms later by
  `committed_transcript_with_timestamps`, which has word times in seconds of
  the audio this session was sent.
- Errors arrive as messages (`auth_error`, `quota_exceeded`,
  `session_time_limit_exceeded`, …) and are followed by a close.
- `previous_text` is accepted only on a session's first chunk.

## Decision

- The tested parameters are kept, and `include_timestamps=true` is added.
  A final is emitted on the timed twin, so its `start_ms`/`end_ms` come
  from its first and last words. A commit whose twin never arrives turns
  final with estimated times.
- Each chunk sent is recorded as an anchor from session time to the side's
  timeline. Word times map through these anchors, so replayed and
  keepalive audio still put captions on the meeting clock. Keepalive
  silence (100 ms after 5 s without audio) maps to no time.
- The adapter keeps the audio since the last final (up to 60 s). On a
  dropped socket, a server error other than auth, quota or input, or a
  session time limit, it reconnects with exponential backoff (0.5 s up to
  8 s). It replays that audio and sends the last ~300 characters of finals
  as `previous_text`. After 5 failures in a row without hearing anything,
  it returns `Network`, so the job falls back. `auth_error` and
  `unaccepted_terms` are `Auth`, `quota_exceeded` is `Quota`, and none of
  them is retried. A refused upgrade or HTTP status maps the same way as
  the batch API: 401/403 is Auth, 402/429 or `detail.status=quota_exceeded`
  is Quota, and 5xx is Network.
- When the audio closes, the adapter sends an empty `commit:true` chunk and
  waits up to 3 s for the last commit. A partial that is still open then
  turns final.
- Batch uploads the side as a 16 kHz mono WAV. Speakers are mapped to
  `S1…` in order of first appearance, and spacing entries are dropped.
- HTTP is `reqwest` 0.12 and websockets are `tokio-tungstenite` 0.30,
  both on native TLS (SChannel). Deepgram and the HTTP AI providers can
  share them.

## Consequences

- The Test button opens a real session and waits for `session_started`
  (about 1 s). It does not spend audio minutes.
- The Fallback replay in the engine and this adapter's own replay are
  separate. The adapter rides out blips, and Fallback handles giving up.
