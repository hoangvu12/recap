# 0003: One audio timeline, and the recording in meeting time

## Context

Me (mic) and Them (process loopback) come from two WASAPI streams with their own buffers. Either can stall: loopback sends nothing while nothing plays, and the mic disappears for a moment when the default device changes. The recording, the live STT and later the click-to-play line all need the two sides on the same clock. The recorder also needs an Opus encoder that builds on MSVC.

## Decision

- **One `Timeline` for every source.** Live capture and the WAV source both push stamped packets (QPC time live, file position for a WAV) into the same `Timeline`. It downmixes, resamples to 16 kHz (rubato FFT), and hands out 100 ms frames for Me and Them in step. A hole in a side is zero-filled; audio stamped for time already heard is dropped; jitter under 20 ms is ignored; a jump over 1 s re-anchors the side. A side that sends nothing is zero-filled once it is 300 ms behind, so the other side keeps flowing.
- **Windows does the conversion live.** Both WASAPI streams ask for 16 kHz mono float with `autoconvert`, so live capture needs no resampler. A device that comes back after a reopen sits at its own QPC time.
- **Reopen, don't stop.** Each capture thread reopens its stream when it is lost (`AUDCLNT_E_DEVICE_INVALIDATED`) or, for the mic, when the default communications device changes. Only the first open's failure is an error from `LiveCapture::start`.
- **The recording is meeting time.** Paused audio is not recorded, so a segment's `start_ms` is its offset in `audio.ogg`.
- **libopus through `opus` 0.4 (bundled, built with CMake).** CMake comes with the VS Build Tools "C++ CMake tools" component, which this machine has; the `cmake` crate finds it without PATH changes. The pure-Rust `opus-rs` port was the alternative but is young; libopus is the reference encoder. Stereo 16 kHz, 32 kbps VOIP, 20 ms frames, a page a second, fsync every 2 s.

## Consequences

- A fresh build machine needs the VS "C++ CMake tools" component (or CMake on PATH).
- The WAV source behaves exactly like live capture, so engine tests on `testdata/meeting_tts.wav` exercise the real alignment code.
- Live captions see Me and Them at most about 300 ms late when one side stalls; otherwise as soon as both sides have the audio.
- A crash loses at most about 2 s of recording (tested: a truncated file decodes up to its last whole page).
