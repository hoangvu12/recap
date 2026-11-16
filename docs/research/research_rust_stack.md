# Rust stack for the Windows 11 meeting notetaker (research, 2026-11-16)

Scope: everything except the UI (zgx is researched separately).

**Method**
- Crate versions come from the crates.io API on 2026-11-16.
- Code was read with `gh api` (crate repos, fastrepl/anarlog, screenpipe, Yap, Chromium).
- Docs: docs.rs, Microsoft Learn, ElevenLabs docs and SDKs, Claude Code docs.
- Throwaway builds under `%TEMP%` (`nt_ws`, `nt_opus`, `nt_opusrs`, `kr_test`, `sessprobe`; all disposable):
  - `cargo check` on wasapi, rubato, cpal and aec3;
  - a built and run Ogg/Opus writer, with a truncation test using ffprobe;
  - tokio-tungstenite + reqwest builds;
  - a keyring + rusqlite run;
  - a read-only probe of audio sessions and ConsentStore.
- **No audio was sent anywhere and ElevenLabs was not called.**
- The only LLM calls were 1-line synthetic prompts ("count the words in 'The quick brown fox.'") to check the CLI output formats.

Confidence levels: **H** = verified in source or docs, or built here. **M** = documented but not run on real devices or meetings. **L** = inferred.

## Recommended crates at a glance

| Part | Crate (version) | Confidence |
|---|---|---|
| 1. Capture | `wasapi` **0.27.0** for both mic and loopback, plus `rubato` **5.1.0**. Pin `windows` **=0.62.2**. | H |
| 2. Echo | Transcript dedup (loqui-style), plus Windows' built-in echo cancellation through `wasapi` 0.27 (`AudioCategory_Communications` + `IAcousticEchoCancellationControl`) where available. `aec3` 0.4.0 as an optional fallback later. | H / M |
| 3. Recording | Ogg/Opus: `opus` **0.4.0** (bundled libopus, built with cmake) + `ogg` **0.9.2**, 32 kbps stereo, about **15 MB/h**. Fallback with no C code: `opus-rs` 0.1.42. | H |
| 4. ElevenLabs | Hand-written client: `tokio-tungstenite` **0.30** (`rustls-tls-native-roots`) + `tokio` 1.54 + `serde_json` + `base64` 0.22. Batch via `reqwest` **0.13.5** (`multipart`, `json`, `stream`). | H |
| 5. Meeting detection | `wasapi` 0.27 audio-session polling (capture endpoints, Active state) + ConsentStore registry (`winreg` 0.56 or `windows` Registry) + window titles via `windows` 0.62.2 | H / M |
| 6. Speaker names | Raw `windows` 0.62.2 `IUIAutomation` (CUIAutomation8 + CacheRequest). `uiautomation` 0.25.1 only for dumping trees. | plumbing H, markers L |
| 7. LLM | `claude -p` (Claude Code 2.1.322) via `std::process::Command`/`tokio::process`. Codex and Gemini CLI are blocked on this machine (see §7). API later via `reqwest`. | H |
| 8. Storage and misc | `rusqlite` **0.43.0** `bundled` (SQLite 3.54.1, FTS5 on), `keyring` **4.2.0** (v1 API → Windows Credential Manager), a named mutex for single instance, `cargo-packager` **0.11.8** (NSIS) | H |

---

## 1. Audio capture (mic + system loopback → 16 kHz mono s16le, common clock)

**Choice: `wasapi` 0.27.0 for both streams.** cpal 0.18.2 *does* do WASAPI loopback: an input stream built on an output device sets `AUDCLNT_STREAMFLAGS_LOOPBACK` ([device.rs](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/wasapi/device.rs)). But cpal does not expose:
- per-packet QPC timestamps;
- the SILENT and DATA_DISCONTINUITY flags;
- per-process loopback (`PROCESS_LOOPBACK` appears nowhere in the repo);
- echo-cancellation control or the stream category.

Since 0.18.0, cpal streams on the *default* device follow default-device changes automatically ([CHANGELOG](https://github.com/RustAudio/cpal/blob/v0.18.2/CHANGELOG.md)). It is unknown whether that also applies to loopback (L).

### wasapi 0.27 key calls
Source: [api.rs](https://github.com/HEnquist/wasapi-rs/blob/v0.27.0/src/api.rs). Every call below passed `cargo check`.

| Step | Call |
|---|---|
| COM, once per capture thread | `wasapi::initialize_mta().ok()?` |
| Device | `DeviceEnumerator::new()?.get_default_device(&Direction::Capture)` for the mic. For endpoint loopback, use the **Render** device with `Direction::Capture`. `get_default_device_for_role(.., Role::Communications)` |
| Init | `let mut ac = dev.get_iaudioclient()?;`<br>`ac.initialize_client(&WaveFormat::new(32,32,&SampleType::Float,48000,2,None), &Direction::Capture, &StreamMode::EventsShared{autoconvert:true, buffer_duration_hns:200_000}, None)?` |
| Loop | `let h = ac.set_get_eventhandle()?; let cc = ac.get_audiocaptureclient()?; ac.start_stream()?;`<br>Then `h.wait_for_event(200)`, and drain with `while cc.get_next_packet_size()?.unwrap_or(0) > 0 { let info = cc.read_from_device_to_deque(&mut deque)?; }` |
| Timestamps | `BufferInfo { flags: { silent, data_discontinuity, timestamp_error }, index /*device position*/, timestamp /*QPC, 100 ns*/ }` |
| **Per-process loopback** | `AudioClient::new_application_loopback_client(pid, include_tree: bool)`. `true` = INCLUDE the process tree; `false` = EXCLUDE it. Example: [record_application.rs](https://github.com/HEnquist/wasapi-rs/blob/v0.27.0/examples/record_application.rs) |
| Device events | `enumerator.register_notification_callback(DeviceEventCallbacks)`, with `set_default_device_callback`, `set_device_state_callback`, added and removed. Keep the returned registration alive. Session events: `EventCallbacks::set_disconnected_callback`. See [device_notifications.rs](https://github.com/HEnquist/wasapi-rs/blob/v0.27.0/examples/device_notifications.rs) |

### Per-process loopback (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`)
- **Platform:**
  - Microsoft lists build 20348 as the minimum ([enum doc](https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ne-audioclientactivationparams-audioclient_activation_type)).
  - OBS enables it from build 19041 ([plugin-main.cpp](https://github.com/obsproject/obs-studio/blob/master/plugins/win-wasapi/plugin-main.cpp)).
  - Windows 11 is fine.
  - Microsoft sample: [ApplicationLoopback](https://github.com/microsoft/Windows-classic-samples/tree/main/Samples/ApplicationLoopback).
- **Not tied to an endpoint:**
  - You get silence while the target isn't playing.
  - It should keep working when the user switches speakers or headphones (M, inferred from the sample).
- **Pitfalls:**
  - You must supply the format yourself.
  - These calls fail in this mode: `get_mixformat`, `is_supported`, `get_device_period`, `get_current_padding`, `get_audioclock`, `get_audiosessioncontrol`.
  - `get_buffer_size` returns garbage.
- **Best default for "system":** `new_application_loopback_client(std::process::id(), false)`. It captures *everything except our own app* and needs no device-switch logic (M; verify that QPC timestamps are filled in this mode).
- **Capturing only the meeting app** (M-L, not tested live):

| Target | What happens |
|---|---|
| Zoom | Include the `Zoom.exe` process tree |
| New Teams | Include the `ms-teams.exe` tree, which covers its `msedgewebview2.exe` children. [quill process_tree.rs](https://github.com/humanitas-labs/quill/blob/HEAD/windows/src/audio/process_tree.rs) does this |
| Teams or Meet in Brave/Chrome/Edge | Audio is played by the browser's audio-service utility process, a child of the browser root. Including the root tree captures **all tabs, YouTube included**. No per-tab isolation |

- **Picking the PID:** find the PID of the Active render session (see §5), then walk up to the process that owns the window.

### Device changes
- **Mic and endpoint loopback:** on `OnDefaultDeviceChanged`, a session disconnect, or `AUDCLNT_E_DEVICE_INVALIDATED` (0x88890004, surfaced as `WasapiError::Windows`), stop the stream, reopen on the new default, and re-anchor the timeline ([MS: stream routing](https://learn.microsoft.com/en-us/windows/win32/coreaudio/stream-routing-implementation-considerations)).
- **Process loopback:** needs nothing.
- **anarlog:** polls the render endpoint every 2 s (`EndpointFollower`) and its Windows device monitor is a stub, so don't copy it.

### Common clock, silence and drift
- **Endpoint loopback goes silent when nothing plays.** Packets stop and timestamps jump; OBS works around it by playing a silent render stream. Instead, write zeros up to "now" on the QPC clock when `wait_for_event` times out or a packet's `timestamp` is ahead of the expected one.
- **Common clock:** both streams carry QPC in 100 ns units ([GetBuffer doc](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer)).
  - Target index = `(qpc - t0) * 16000 / 1e7`.
  - Drop samples if a stream is more than ~20 ms ahead; pad with zeros if it is behind.
  - Resync on `data_discontinuity`.
- **Drift:** about 50 ppm, roughly 3 ms/min or 180 ms/h. That doesn't matter for separate-channel STT; it only matters for echo cancellation. Fixes, simplest first:
  1. drop or insert against the QPC timeline;
  2. `rubato::Slip`;
  3. `Async` + `set_resample_ratio_relative(rel, true)` driven by a small PI loop.
- **rubato 5.1 API** ([docs.rs](https://docs.rs/rubato/5.1.0)). It changed from older versions; buffers come from `audioadapter`.
  - `Fft::<f32>::new(48000, 16000, chunk, 1, FixedSync::Input)` for a fixed ratio.
  - `Async::new_poly(ratio, max_rel, PolynomialDegree::Cubic, chunk, ch, FixedAsync::Input)` when drift correction is needed.
  - `process_into_buffer(&InterleavedSlice::new(&buf, ch, frames)?, &mut out, None)`.
  - **Downmix to mono yourself first** (average the channels). anarlog keeps only channel 0, which is a bug risk.

### anarlog reference (fastrepl/anarlog HEAD)
- It uses `wasapi` 0.23, `cpal` 0.17 and `rubato` 1.0.1.
- Speaker capture: `crates/audio-actual/src/speaker/windows.rs`. Endpoint loopback, 44.1 kHz float, channel 0 only, ignores timestamps and flags.
- Mic: `mic.rs` (cpal), with device ranking and virtual-device filtering.
- **Pairs chunks by count, not by timestamp:** `capture/joiner.rs` pads the lagging side with silence when it is more than 4 chunks behind.
- Recovery: `recovery.rs`.
- Takeaway: use it for structure, but do the clock alignment better (QPC).

---

## 2. Echo (laptop speakers → the mic hears the remote side)

| Option | Facts | Verdict |
|---|---|---|
| **Transcript dedup** ([loqui dedup.py](https://github.com/joaquingit1/loqui/blob/HEAD/sidecar/loqui_sidecar/dedup.py)) | Drops a **mic** segment if it overlaps a **system** segment in time (±1.75 s) and either the difflib ratio is ≥ 0.85 or ≥ 90% of the mic text appears in the system text. Never drops system segments. Keeps short replies ("ok", "yeah": under 12 chars or under 3 tokens). | **Always do this.** Zero DSP risk, works on any hardware. Run it on live segments and again on the batch result. H |
| **Windows built-in echo cancellation** | Steps, all exposed by wasapi 0.27 (no raw `windows` calls needed):<br>1. `AudioClientProperties::new().set_category(StreamCategory::Communications)` → `ac.set_properties()` **before** `initialize_client`, on the `Role::Communications` capture device.<br>2. `ac.is_aec_supported()?`<br>3. `ac.get_aec_control()?.set_echo_cancellation_render_endpoint(Some(render_id))`.<br>Example: [examples/aec.rs](https://github.com/HEnquist/wasapi-rs/blob/v0.27.0/examples/aec.rs).<br>`IAcousticEchoCancellationControl` needs build 22621 or later ([doc](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nn-audioclient-iacousticechocancellationcontrol)). It exists only if the OEM's or Windows' Voice Clarity processing provides echo cancellation; otherwise `E_NOINTERFACE`. | Use when available and the output isn't headphones. **Pitfalls:**<br>• Communications streams trigger **ducking**: opt out with `set_ducking_preference(true)`.<br>• It also adds noise suppression and gain control.<br>• Quality varies by OEM. M |
| `aec3` 0.4.0 ([aec3-rs](https://github.com/RubyBit/aec3-rs)) | Pure-Rust port of WebRTC AEC3, 10 ms frames, builds on MSVC (checked). The README calls it work in progress; 62 stars. | Optional fallback later. Needs the QPC-aligned loopback as reference. M-L |
| `webrtc-audio-processing` 2.1.0 | `bundled` needs meson, ninja and pkg-config. CI covers Linux and macOS only. MSVC support is unmerged ([#34](https://github.com/tonarino/webrtc-audio-processing/issues/34), [PR #102](https://github.com/tonarino/webrtc-audio-processing/pull/102) still open). | Avoid on Windows. H |
| anarlog ONNX echo cancellation (`crates/aec/src/onnx`) | DTLN-aec-style model pair, plus a cross-correlation lag probe up to 600 ms (`crates/audio-sync`) and a linear residual canceller. Needs `ort` and about 3 MB of models. | Too heavy for v1 |

- **Headphone check** (from anarlog `crates/audio-device/src/windows.rs`): the device name contains headphone/headset/earphone/airpods, or it's Bluetooth (`bth*`) and the name doesn't mention a speaker. Skip echo cancellation only if *all* active outputs are headphones.
- **Simplest robust approach:** dedup always, plus Windows echo cancellation when supported and not on headphones.

---

## 3. Local recording (ch0 mic, ch1 system), crash-safe

| Format | Crate | Build on MSVC | Crash safety | Size per hour |
|---|---|---|---|---|
| **Ogg/Opus** (recommended) | `opus` 0.4.0 (via `opusic-sys`; feature `bundled` compiles libopus) + `ogg` 0.9.2 | Built here with no extra setup: the `cmake` crate found the CMake bundled with VS Build Tools. It fails without the VS "C++ CMake tools" component. No NASM. | Every Ogg page has a CRC, so a partly written last page is dropped. **Test:** a 60 s file cut at 150 KB still decoded 36 s cleanly. | 32 kbps stereo VOIP: **14.8 MB/h measured**, about 30 MB for 2 h |
| Opus without C | `opus-rs` 0.1.42 (restsend, pure Rust port of libopus 1.6) | Nothing needed | Same | Same. The repo is young (2026-02, 56 stars); worked here |
| FLAC | `flacenc` 0.5.1 (pure Rust, `default-features=false`) | Nothing needed | Incremental use: write the header with total_samples=0, then `encode_fixed_size_frame` per block. Patch STREAMINFO on a clean stop. | ~90–140 MB/h (L) |
| WAV | `hound` 3.5.1 | Nothing needed | `WavWriter::flush()` rewrites the header sizes but never fsyncs. Either wrap the `File` so that `flush` also calls `sync_data`, or repair the header on next start. | 230 MB/h |

**Ogg/Opus writing recipe** (tested):
- `Encoder::new(16000, Channels::Stereo, Application::Voip)` and `set_bitrate(Bitrate::Bits(32000))`.
- Use 20 ms frames (320 samples per channel).
- Write **OpusHead** with `pre_skip` = lookahead × 3 (in 48 kHz units), input rate 16000, mapping family 0. Then write **OpusTags**. Both on their own pages.
- **Granule positions are counted at 48 kHz** (+960 per 20 ms frame).
- Every 50 packets (1 s): `PacketWriteEndInfo::EndPage`.
- Every 1–5 s: `flush()` + `sync_data()`.
- On a clean stop: `EndStream`.

**STT acceptance:**
- **ElevenLabs batch** accepts `audio/ogg` and `audio/opus` (up to 5 GB and 15 h). The multichannel guide doesn't explicitly name stereo Ogg/Opus, so test once (M).
- **Deepgram** accepts Ogg/Opus with `multichannel=true`.

---

## 4. ElevenLabs Scribe v3 Live (realtime) and batch, in Rust

**SDKs:** write the client yourself.
- `elevenlabs_rs` 0.7.1 has realtime STT, but it has no `turn_prediction`, no v3 Live model, wrong error tags (`scribe_auth_error` vs the server's `auth_error`), and no diarization events.
- `elevenlabs` 0.4.2 is a CLI.
- The protocol is about 150 lines of code (H).

### Realtime protocol
Sources: [API ref](https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime), [events](https://elevenlabs.io/docs/eleven-api/guides/how-to/speech-to-text/realtime/event-reference), [commit strategies](https://elevenlabs.io/docs/eleven-api/guides/how-to/speech-to-text/realtime/transcripts-and-commit-strategies), [Python SDK scribe.py](https://github.com/elevenlabs/elevenlabs-python/blob/main/src/elevenlabs/realtime/scribe.py).

**Connection**
- URL: `wss://api.elevenlabs.io/v1/speech-to-text/realtime?...`
- Auth: header `xi-api-key`, or `token=` with a single-use token from `POST /v1/single-use-token/realtime_scribe` (15 min).
- `model_id=scribe_v3_live` is officially listed; `turn_prediction` works **only** on v3 Live.

**Query parameters**
- Already used: `audio_format=pcm_16000`, `commit_strategy=turn_prediction|vad|manual`, `language_code`, `no_verbatim`, `keyterms` (repeated, ≤ 50 × 20 chars).
- Also available:
  - `include_timestamps`, `include_language_detection`;
  - VAD tuning: `vad_silence_threshold_secs`, `vad_threshold`, `min_speech_duration_ms`, `min_silence_duration_ms`;
  - **`diarize`** (v3 Live; not combinable with `filter_background_audio`);
  - **`keepalive_interval_ms`** (500–10000);
  - `enable_logging`;
  - entity and redaction options.
- **No multichannel** in realtime: open one socket per stream (M).

**Client → server**
- Only `{"message_type":"input_audio_chunk","audio_base_64":"…","commit":false,"sample_rate":16000}`.
- `previous_text` is allowed **only on the first chunk** (≤ ~50 characters is best; intended for reconnects).
- Chunks of 0.1–1 s.

**Server → client**
- Transcripts:
  - `session_started{session_id,config}`;
  - `partial_transcript{text}`;
  - `committed_transcript{text, segment_index?, turn_probability?}`;
  - `committed_transcript_with_timestamps{text, words[{text,start,end,type,speaker_id,logprob}]}`.
- With `diarize`: `diarized_transcript{segment_index,text,words}` and `speakers_merged{from_speaker_id,to_speaker_id}`.
- Other: `warning`.
- **Errors** are all shaped `{message_type, error}` and are followed by a close: `auth_error`, `quota_exceeded`, `rate_limited`, `queue_overflow`, `resource_exhausted`, `session_time_limit_exceeded`, `input_error`, `invalid_request`, `chunk_size_exceeded`, `insufficient_audio_activity`, `transcriber_error`, `commit_throttled`, `unaccepted_terms`, `error`.

**Limits**
- The server closes after **15 s without client messages**. This is not configurable; keep sending silence chunks.
- The maximum session length is undocumented (only the `session_time_limit_exceeded` error exists), so reconnect handling is required.

### Rust websocket
`tokio-tungstenite = { version = "0.30", features = ["rustls-tls-native-roots"] }` (built here).
- **TLS:** rustls 0.23.46 defaults to aws-lc-rs with prebuilt NASM, which built on MSVC without cmake or NASM. Don't enable `ring` as well, or you must call `install_default()`. Native roots read the Windows certificate store, which works behind corporate TLS inspection.
- **Connect:**
  - `let mut req = url.into_client_request()?;`
  - `req.headers_mut().insert("xi-api-key", HeaderValue::from_str(&key)?);`
  - `let (ws,_) = connect_async(req).await?;`
  - `let (tx, rx) = ws.split();`
- **Tasks:** a sender task drains an mpsc of 100 ms PCM chunks (base64 → `Message::Text(json.into())`); a receiver task deserializes the `#[serde(tag="message_type")]` enum. Parse into a `Value` first: if it has an `error` key, it's an error.
- **Reconnect without losing audio:**
  1. Keep a ring buffer of the chunks sent since the last `committed_transcript`. It is bounded by the ~36 s auto-commit, about 1.2 MB.
  2. On reconnect, send the first chunk with `previous_text` = the last ~50 committed characters, replay the buffer, then go live.
  3. Drop a new commit whose start overlaps the tail of the previous commit.
  4. `segment_index` restarts each session, so key it by session.
  5. Retry with backoff on `session_time_limit_exceeded`, `resource_exhausted`, `queue_overflow` and network errors. **Never** retry on `auth_error`, `quota_exceeded` or `unaccepted_terms`.

### Batch STT
`POST https://api.elevenlabs.io/v1/speech-to-text`, multipart ([convert ref](https://elevenlabs.io/docs/api-reference/speech-to-text/convert), [multichannel](https://elevenlabs.io/docs/eleven-api/guides/how-to/speech-to-text/batch/multichannel-transcription)).

**Fields**
- `model_id=scribe_v3` (v1 is deprecated).
- `file` (or `source_url`), `language_code`.
- `diarize`, `num_speakers` (≤ 32), `diarization_threshold`.
- `timestamps_granularity=word`.
- `use_multi_channel` + `multichannel_output_style=separate|combined`.
- `keyterms`: a **repeated form field**, ≤ 1000 terms of ≤ 50 chars, +20% price.
- `no_verbatim`, `tag_audio_events`.
- `webhook=true` (+ `webhook_id`, `webhook_metadata`) returns early with `{request_id, transcription_id}`.

**⚠ `use_multi_channel` requires `diarize=false`.** The speaker becomes `speaker_<channel>` and each channel is billed for the full duration. So a 2-channel file gives "me vs them" but **does not split the remote speakers**. To diarize the remote side, either:
- send the **system channel as its own mono file with `diarize=true`**, plus the mic channel mono without diarize (same cost as 2 channels), or
- use Deepgram, whose `multichannel=true&diarize=true` diarizes per channel.

**Response**
- Single channel: `{language_code, language_probability, text, words:[{text, type: word|spacing|audio_event, start, end, speaker_id, logprob, characters?}], transcription_id}`.
- Multichannel: `{transcripts:[{channel_index, text, words…}], transcription_id, audio_duration_secs}`.
- Deserialize with `#[serde(untagged)] enum { Multi{transcripts}, Single }`, Multi first.

**reqwest 0.13.5** (features: default rustls + platform verifier, plus `multipart`, `json`, `stream`):
```rust
let part = reqwest::multipart::Part::file(path).await?.file_name("system.ogg").mime_str("audio/ogg")?;
let form = reqwest::multipart::Form::new()
    .text("model_id", "scribe_v3").text("diarize", "true").text("timestamps_granularity", "word")
    .text("keyterms", "Foo").text("keyterms", "Bar").part("file", part);
let r: SttResponse = client.post("https://api.elevenlabs.io/v1/speech-to-text")
    .header("xi-api-key", key).multipart(form).send().await?.error_for_status()?.json().await?;
```

---

## 5. Meeting detection

Mirror Wispr's `session_plus_registry` design (research_wispr.md §1):

| Signal | How (Rust) | Notes |
|---|---|---|
| **Mic in use, and by which exe** | `wasapi` 0.27: `DeviceEnumerator::get_device_collection(&Direction::Capture)` → `get_iaudiosessionmanager()` → `get_audiosessionenumerator()` → `get_state()==Active` → `get_process_id()`. Poll every 1 s, as anarlog's `crates/detect/src/mic/windows.rs` does. Push alternative: `IAudioSessionNotification` + `IAudioSessionEvents::OnStateChanged` with `#[implement]`. | Sessions stay listed after they stop, so **filter on Active**. A browser tab's session belongs to the browser's **audio-service utility process** (`brave.exe --utility-sub-type=audio.mojom.AudioService`, confirmed here), so it doesn't identify the tab. Raw `windows` features: `Win32_Media_Audio`, `Win32_System_Com`, **`Win32_System_Com_StructuredStorage` + `Win32_System_Variant`** (otherwise `IMMDevice::Activate` isn't generated). H |
| **ConsentStore cross-check** | `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\{<PackageFamilyName> \| NonPackaged\<path with # for \>}`, QWORD FILETIME values `LastUsedTimeStart` / `LastUsedTimeStop`; **Stop == 0 means in use now**. Read with `winreg` 0.56. Watch with `RegNotifyChangeKeyValue(h, TRUE, LAST_SET\|NAME\|THREAD_AGNOSTIC, event, TRUE)`; it is one-shot, so re-arm after each signal and re-enumerate ([MS Learn](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regnotifychangekeyvalue)). | Verified read-only here: about 100 NonPackaged entries. **The HKLM equivalent is empty; use HKCU.** One entry per exe, not per tab. Undocumented key. A crash can leave Stop == 0, so cross-check with sessions. Match on the exe basename, because versioned folders such as Discord's `app-1.0.x` pile up. H / M |
| **Which platform** | `EnumWindows` + `GetWindowTextW` + `GetClassNameW` + `GetWindowThreadProcessId` (`Win32_UI_WindowsAndMessaging`) | **Meet:** title `Meet - xxx-xxxx-xxx` (anarlog `meeting_ax/platform.rs`).<br>**Teams web:** title contains "Microsoft Teams".<br>**New Teams:** `ms-teams.exe` (+ `msedgewebview2.exe`).<br>**Zoom:** title "Zoom Meeting", class `ConfMultiTabContentWndClass` (older builds: `ZPContentViewWndClass`); the Workplace main window shares the class.<br>Only the active tab's title is visible, and titles are localized. M |
| **In-call confirmation** (optional) | UIA: a "Leave call" / "Hang up" / "Leave meeting" button, or the `hangup-button` AutomationId | anarlog `meeting_ax/windows.rs`, screenpipe `meeting_watcher/shared/profiles.rs` |

---

## 6. Speaker names from the meeting UI (UI Automation)

**API:** raw `windows` 0.62.2 `IUIAutomation`.
- `CoCreateInstance(&CUIAutomation8)`, then `cast::<IUIAutomation2>()`, then `SetConnectionTimeout` / `SetTransactionTimeout` (2 s).
- `CreateCacheRequest` with Name, ClassName, AutomationId, AriaProperties, ItemStatus and IsOffscreen, `TreeScope_Subtree`, `RawViewCondition`.
- **One** `ElementFromHandleBuildCache` or `FindAllBuildCache` per poll. Both anarlog and Yap do this.
- Avoid `uiautomation`'s `UIMatcher`: it walks with a client-side TreeWalker, so it makes many cross-process calls.
- `uiautomation` 0.25.1 also initializes COM in the MTA on every `new()` (`RPC_E_CHANGED_MODE` on STA threads). Fine for dumping trees.

**Chromium, Brave, Edge, WebView2 (new Teams)**
- **Turning it on:**
  - The UIA tree is enabled lazily, the first time a UIA client calls `WM_GETOBJECT` and reads properties (`legacy_render_widget_host_win.cc`, `ax_platform_node_win.cc`).
  - So **the first poll may be nearly empty**: retry after about 1 s.
  - Native UIA is on by default from Chrome 138 ([Chrome blog](https://developer.chrome.com/blog/windows-uia-support-update)).
  - Brave uses the same code (M).
- **Hidden tabs:** Chromium turns accessibility **off** for a tab hidden for 5 minutes or more (`AccessibilityDisabler`). Background tabs and minimized windows then show nothing; treat that as "unknown".
- **Cost:** turning accessibility on costs the browser renderer CPU and memory.

**What the meeting UIs expose** (from Yap's [`profiles.json`](https://github.com/nayballs/Yap/blob/HEAD/desktop/src-tauri/src/speakers/call_ui/profiles.json), which says the markers came from Wispr's remote defaults and were *not checked against a live call*):

| Platform | Markers |
|---|---|
| Meet | Speaking class `kssMZb` on tile `oZRSLe` (obfuscated, changes when Google redeploys); text "{name} is speaking"; "(You)" marks yourself |
| Teams | `vdi-frame-occlusion`; "{name}, speaking"; roster items like "Name, Muted, Attendee" |
| Zoom 7 | No control IDs, localized names; "Active speaker"; participants readable only with the panel open ([ZoomHelper.cs](https://github.com/sircharlo/meeting-media-manager/blob/HEAD/src-electron/zoom-helper/ZoomHelper.cs)) |

**Robustness: low to medium.**
- **Fragility:** markers break when the apps update, and text is localized. Ship the markers as data the app can update without a rebuild (Yap uses an override JSON) and keep a tree-dump tool for capturing fixtures.
- **Polling:** every 0.5–1 s, on a dedicated MTA thread, with a cap of about 4k–30k nodes, only while a meeting is detected.
- **Use as votes:** the highlight lags speech by 0.5–1 s, so treat it as votes per diarized cluster by time overlap, not as ground truth.
- **Untested idea:** read the Meet/Teams live-caption region instead (name + text), which may be steadier (L).

---

## 7. LLM calls without an API key (installed CLIs)

What was checked on this machine on 2026-11-16, with 1-line synthetic prompts:

| CLI | Status here | Headless recipe |
|---|---|---|
| **Claude Code 2.1.322** (`~/.local/bin/claude.exe`, a native exe) | **Works.** Output: `{"type":"result","subtype":"success","is_error":false,"result":"…","structured_output":{…},"total_cost_usd":…,"usage":{…},"session_id":…}` | `claude -p "<instructions>" --output-format json --json-schema '<schema>' --tools "" --no-session-persistence --model sonnet` (or `opus`).<br>**Transcript on stdin** (`cat t.txt \| claude -p "…"`; capped at **10 MB**, about 100× an hour-long transcript).<br>`--restricted` ignores user, project and local settings and removes code-running tools.<br>Run from an empty temp cwd so no `CLAUDE.md` or hooks load.<br>**Don't use `--bare`**: it reads *only* `ANTHROPIC_API_KEY` and never the subscription login.<br>Check the `structured_output` field is present (it's missing on refusal). Exit code ≠ 0 on failure.<br>Timeout: `tokio::time::timeout(180 s)` + `kill_on_drop(true)`. ([headless docs](https://code.claude.com/docs/en/headless)) |
| **Codex CLI 0.128.0** | **Auth expired** ("access token could not be refreshed"); needs `codex login`. It is an npm shim (`codex.ps1`/`.cmd`), so spawn the native `…\npm\node_modules\@openai\codex\node_modules\@openai\codex-win32-x64\vendor\x86_64-pc-windows-msvc\codex\codex.exe` directly; Rust `Command` doesn't resolve `.cmd`/`.ps1`. | `codex exec --skip-git-repo-check --ephemeral -s read-only --output-schema schema.json -o last.json --json "<instructions>"` with the transcript piped on stdin (it is appended as a `<stdin>` block; `codex exec -` reads the whole prompt from stdin).<br>JSONL events: `thread.started`, `turn.started`, `item.completed{item:{type:"agent_message",text}}`, `turn.completed{usage}`, `turn.failed`, `error`. ([docs](https://learn.chatgpt.com/docs/non-interactive-mode)) |
| **Gemini CLI 0.47.0** | **Blocked:** `IneligibleTierError UNSUPPORTED_CLIENT`, "This client is no longer supported for Gemini Code Assist for individuals… migrate to Antigravity". Also needs `--skip-trust` headless. | `gemini -p "…" -o json` returns `{response, stats}`. It doesn't work with this account any more. |
| **Antigravity CLI `agy` 1.5.5** (installed, `%LOCALAPPDATA%\agy\bin\agy.exe`) | **Works.** Models include `gemini-3.8-flash-*`, `claude-sonnet-4-6`, `claude-opus-4-6-thinking`, `gpt-oss-120b`. Output: `{status:"SUCCESS", response, structured_output, usage, denied_actions}`. | `agy -p "<prompt>" --output-format json --json-schema '<schema>' --mode plan --model gemini-3.8-flash-medium --print-timeout 300s`.<br>**It did not use piped stdin**: it tried a shell command, which headless mode auto-denied. Put the transcript in a file in a temp workspace and reference it (M, untested), or in argv (Windows command lines are limited to 32 KB). Terms not reviewed. |

**Subscription terms** ([Claude Code legal](https://code.claude.com/docs/en/legal-and-compliance)):
- Pro/Max OAuth "is designed to support ordinary use of Claude Code". Advertised limits "assume ordinary, individual usage of Claude Code and the Agent SDK".
- Anthropic does **not** permit third-party developers to "offer Claude.ai login" in their apps or to "route requests through Free, Pro, Max, or Ultra plan credentials on behalf of their users". Apps may not "collect, store, or intermediate" Claude.ai credentials.
- **Reading:** a personal tool that spawns the user's own unmodified `claude` binary, signed in by the user, is the "end user signing in to the unmodified Claude Code binary" case. That is acceptable for **personal use only**. **Do not distribute** the app with this as its LLM path; for that, use an API key. Two calls per meeting is well within ordinary usage.
- OpenAI also says API keys are "the right default for automation". Using ChatGPT login in automation is an "advanced" path for your own account only.

**Pitfall seen here:** an inherited `ANTHROPIC_API_KEY` env var **takes precedence** over the subscription login. The test call was billed to the API key ($0.0013). The app must `cmd.env_remove("ANTHROPIC_API_KEY")` (and `ANTHROPIC_AUTH_TOKEN`) to use the subscription, or set it on purpose to use the API.

**Direct API later:** raw `reqwest` (there's no official Rust SDK).
- `POST https://api.anthropic.com/v1/messages`.
- Headers: `x-api-key`, `anthropic-version: 2026-10-21`, `content-type: application/json`.
- Body: `{"model":"claude-sonnet-5-5","max_tokens":16000,"output_config":{"effort":"low","format":{"type":"json_schema","schema":{…}}},"system":[{"type":"text","text":"<stable instructions>","cache_control":{"type":"ephemeral"}}],"messages":[{"role":"user","content":"<transcript>"}]}`.
- Read `content[0].text` (JSON) and check that `stop_reason != "refusal"`. Don't send `temperature` (rejected on 2026-10-21).
- Prices per million tokens, input/output: Haiku 5.5 $0.10/$0.50, Sonnet 5.5 $2/$10, Opus 5.5 $4/$20.
- A one-hour meeting (about 15–20k tokens in, 2k out) costs about **$0.06 on Sonnet 5.5** and well under $0.01 on Haiku 5.5.

---

## 8. Storage and misc

**rusqlite 0.43.0 `features=["bundled"]`**
- Gives SQLite 3.54.1 with FTS5 compiled in (checked here). Open with `PRAGMA journal_mode=WAL; synchronous=NORMAL; foreign_keys=ON`.
- Keep the DB and per-meeting folders in `%LOCALAPPDATA%\<App>\` (`directories` 6.0).

Schema sketch:
```sql
meetings(id TEXT PK, title, platform, app_exe, started_at, ended_at, audio_path, status /*recording|refining|done|failed*/, calendar_ref, created_at);
speakers(id INTEGER PK, meeting_id FK, label /*me|S1001…*/, channel, display_name, name_source /*self|llm|uia|manual*/, confidence, contact_id NULL);
segments(id INTEGER PK, meeting_id FK, source /*live|refined*/, channel, speaker_id FK NULL, t0_ms, t1_ms, text, words_json NULL, stt_session, segment_index, is_final, superseded_by NULL);
speaker_observations(meeting_id, t_ms, kind /*active_speaker|participant|muted|self*/, name, platform_source);  -- UIA votes, kept local
notes(meeting_id PK, user_notes_md, summary_json, summary_md, model, generated_at);
contacts(id PK, name, aliases_json);  keyterms(id PK, term, scope /*global|meeting*/);
CREATE VIRTUAL TABLE segments_fts USING fts5(text, content='segments', content_rowid='id');
```
Also keep `live.ndjson` and `refined.ndjson` in the meeting folder, append-only, as in Wispr; the database can be rebuilt from them.

**Secrets: `keyring` 4.2.0** (default `v1` feature; Windows store `windows-native-keyring-store` 1.1)
- `keyring::Entry::new("app", "elevenlabs")?.set_password(k)?`, `.get_password()?`, `.delete_credential()?`. Round trip verified here.
- Entries show in Credential Manager → Windows Credentials.
- Any process running as the same user can read them (DPAPI per user). Fine for this threat model.
- Remember to **rotate the ElevenLabs key** that was pasted in chat.

**Single instance**
- Named mutex: `CreateMutexW(None, true, w!("Local\\<AppName>-singleton"))`, then `GetLastError() == ERROR_ALREADY_EXISTS` → hand off to the first instance and exit (`windows` `Win32_System_Threading`).
- Hand-off: a named pipe via `interprocess` 2.4.8 to send "show window" or "start recording".
- The `single-instance` crate (0.3.3, 2021) is unmaintained but works the same way.

**Installer**
- `cargo-packager` **0.11.8** (crabnebula; NSIS `.exe` or WiX `.msi`; per-user NSIS install avoids UAC).
- Alternatives: `cargo-wix` 0.3.9 (MSI only, last release 2025-03) or `cargo-dist` 0.34.0.
- For a personal tool, a zip or `cargo install --path .` plus a Start-menu shortcut and an `HKCU\…\Run` autostart value is enough.

**Code signing**
- Unsigned builds trigger a SmartScreen warning on first run (once per new binary hash, for files with mark-of-the-web).
- [Azure Artifact Signing](https://www.devclass.com/security/2026/01/14/code-signing-windows-apps-may-be-easier-and-more-secure-with-new-azure-artifact-service/4079554) (formerly Trusted Signing) costs $9.99 a month, but individual developers must be in the **US or Canada** ([pricing/eligibility](https://my-ssl.com/learn/azure-trusted-signing-vs-code-signing-certificate)). So it's likely unavailable to this user; EV/OV certificates cost $300–900 a year.
- For personal use, skip signing. Locally built binaries carry no mark-of-the-web anyway.

---

## Top risks
1. **ElevenLabs multichannel and diarization are exclusive** in batch, and realtime has no multichannel.
   - Remote-speaker separation needs the system channel sent as its own diarized stream, or Deepgram.
   - Two live sockets double realtime usage: the Starter plan's 27 h becomes about 13 meeting-hours a month.
2. **Speaker names from the meeting UI are fragile.** The markers haven't been checked against a live call; Meet and Teams classes are obfuscated and change; Zoom exposes no IDs; hidden tabs lose accessibility. Treat them as optional votes.
3. **Using LLM subscriptions from the app.** Fine for personal use with the user's own `claude` login; not distributable.
   - Gemini CLI personal OAuth is dead (it says migrate to Antigravity). Codex needs a re-login.
   - An inherited `ANTHROPIC_API_KEY` silently switches Claude Code to API billing.
4. **Capture edge cases:**
   - endpoint loopback goes silent when nothing plays (fill from QPC);
   - per-process loopback includes every browser tab;
   - it's unverified whether QPC timestamps are filled in process-loopback mode;
   - the default-device switch must be handled for the mic and endpoint loopback.
5. **Windows built-in echo cancellation depends on the hardware** (`E_NOINTERFACE` on many laptops) and triggers ducking. Transcript dedup is the guaranteed baseline.
6. **Build prerequisites:** `opus` needs CMake (the VS "C++ CMake tools" component). Pin `windows = "=0.62.2"`: `windows-core` and `windows-registry` jumped to 0.100 with breaking renames, and `wasapi` and `uiautomation` depend on 0.62.
