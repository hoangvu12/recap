# 0021 Echo dedup by level, before text

Extends ADR 0009 (echo dedup by text similarity).

## Context

On real calls, Notes showed "Me" rows while the user said nothing. With speakers rather than a headset, the mic picks up the meeting audio (bleed). The STT then transcribes it again as Me, often garbled, especially when the language is mixed (ADR 0022). The text rule (±1.75 s, similarity ≥ 0.85) misses these rows because the words differ from Them's.

We measured this on two of the user's own meetings, decoded locally and never kept:

- **Meeting 1:** Them at -30 to -40 dBFS. Bleed in the mic at about -60 dBFS: a gain of about -26 dB, bursts up to -18 dB, lagging about 200 to 250 ms. The waveform correlation between the two was low (0.1 to 0.3), so correlation is not a usable signal. The user's own speech was -32 to -48 dBFS.
- **Meeting 2:** Them was loud (-10 to -25). The mic was noise-gated to digital silence between words.

Windows echo cancellation (the communications-mode audio processing) applies only to streams that ask for it, as Teams' own stream does. Recap opens the default communications mic, but as a plain shared-mode stream with no communications category, so it gets the mic before any echo cancellation. Even so, the cancellation only removes the audio of the stream it is paired with, and is a driver feature that many laptops lack, so Recap can't rely on it.

## Decision

`recap_engine::mic_levels::MicLevels` judges each Me segment by levels in 100 ms frames:

- **A frame is mine** when the mic is both:
  - above the echo threshold: Them's peak over the last 5 frames, plus the meeting's echo gain (median mic−Them gain + 12 dB, clamped to [-24, -6] dB; -18 dB until 30 frames are known);
  - at least 8 dB above the mic's floor (its 10th percentile).
- **A Me segment is echo** when all of these hold:
  - Them spoke during it (peak above -50 dBFS);
  - at least 300 ms of it has known levels;
  - less than 300 ms of it is mine, or less than 20% of it is mine.
- **Live.** The engine pushes every frame that isn't paused. A Me final that is echo is dropped before the text rule runs, and its partial is cleared.
- **After Stop.** `refine::turns_with` takes the levels built from `audio.ogg` and drops the same Me segments.
- **Unknown levels drop nothing.** If there are no frames, or the meeting audio was silent, only the text rule applies.

## Consequences

- Checked on the two real meetings with `cargo run -p recap-engine --example echo_check -- <folder>`, which prints times and verdicts only:
  - Meeting 2 live: the user's real greeting and one other line are kept; 11 echo rows are dropped, including two long garbled ones.
  - Meeting 1: its one Me row was bleed, and it is dropped.
- When both people talk at once, Me survives only if the mic is clearly louder than the bleed. A quiet user over loud speakers may lose a short interjection. That is better than invented Me rows.
- Use a headset to avoid bleed altogether.
