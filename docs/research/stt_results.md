# Live captions engine test — results (2026-11-16)

**Decision (user): use ElevenLabs Scribe v3 Live for live captions.** Open-source testing stopped and the local model folders were deleted. Parakeet came close on accuracy but couldn't run in real time on this laptop under Windows (details below).

## Method
- Replay `meeting.ogg` (7.7 min, 16 kHz mono; as `oss_live/meeting16k.wav`) through each system's **streaming** path. ElevenLabs is fed 100 ms chunks at real-time pace. Local models use a simulated live clock: a chunk becomes available at its audio end time, and the clock advances by compute time, so any backlog carries over.
- **Accuracy** = word disagreement after normalisation, not true WER. Identical runs differ by about ±0.7 pt.
  - Full 7.7 min: `compare.py` with `whisper_large_v3.txt` as the reference, and again with `dg_en_kt.txt`.
  - 6.5-min stretch: `versus_oss.py`, a copy of `wispr_note/versus.py` that accepts extra `label=path` args. Its "vs Wispr live" column uses Wispr live (Soniox) as the reference.
- **Latency** (`harness/latency.py`):
  - Hypothesis words are aligned to Deepgram whole-file word timings (`dg_en_kt.json`).
  - *seen* = from word end until the word first appears on screen and stays unchanged.
  - *final* = from word end until the word is committed as final.

## Results

| System | vs Whisper, 7.7 min | vs Deepgram file, 7.7 min | vs Whisper, 6.5 min | vs Wispr live, 6.5 min | seen median / p90 | final median / p90 |
|---|---|---|---|---|---|---|
| Wispr live (Soniox, via Wispr) | — | — | 6.8% | 0 | — | — |
| Wispr refined (after meeting) | — | — | 7.2% | 6.4% | — | — |
| Deepgram whole file (after meeting) | 8.5-10.3% | 0 | 8.3% | 7.7% | — | — |
| **ElevenLabs v3 Live + keyterms + no_verbatim, turn_prediction** | **9.5%** | 9.3% | **8.6%** | **8.4%** | **0.69 / 2.0 s** | 16.9 / 31.9 s |
| ElevenLabs v3 Live + keyterms + no_verbatim, vad 0.5 s | 9.9% | 10.1% | 9.3% | 9.0% | 0.71 / 1.9 s | 7.7 / 25.8 s |
| ElevenLabs v3 Live + keyterms, vad 0.5 s | 10.9% | 10.1% | 9.9% | 9.1% | 0.68 / 1.8 s | 7.8 / 25.8 s |
| ElevenLabs v3 Live + keyterms, turn_prediction | 10.9% | 8.9% | 9.6% | 8.5% | 0.70 / 2.0 s | 17.5 / 32.1 s |
| ElevenLabs v3 Live, turn_prediction | 11.3% | 9.5% | 10.6% | 9.3% | 0.70 / 2.2 s | 17.1 / 32.0 s |
| ElevenLabs v2 Realtime, vad | 13.2% | 11.6% | 12.1% | 11.0% | 1.12 / 3.0 s | 17.3 / 32.1 s |
| Our live (Deepgram Nova-3), ep800 / ep300 | 12.4% / 11.0-12.4% | 6.5-7.0% | 11.5% / 11.2% | 10.4% | 0.86 / 2.5 s, 0.80 / 2.3 s | 3.3 / 4.9 s, 3.1 / 4.9 s |
| Parakeet-unified-en-0.6b, sherpa-onnx int8 560 ms, CPU | 7.6% | 10.4% | 7.2% | 7.9% | n/a: 3.3× slower than real time | n/a |
| Parakeet TDT 0.6B v3, sherpa-onnx int8, CPU 4 threads: Silero VAD segments (0.5 s pause, max 20 s), partials re-decoded every 1 s | 14.3% | 14.0% | 15.9% | 15.6% | 4.3 / 14.3 s | 7.5 / 18.6 s (RTF 0.48) |
| Parakeet v3, same, whole file (after-meeting) | 14.3% (identical segments) | 14.0% | 15.9% | 15.6% | — | RTF 0.11 (7.7 min in ~49 s) |
| Qwen3-ASR 1.7B (WhisperLiveKit qwen3-streaming) | hung loading checkpoint on Windows, no result | | | | | |

Notes:
- **"final" for ElevenLabs** = the `committed_transcript` time. With turn_prediction it commits at speaker turns, or after about 36 s at most.
  - The partial text is stable early ("seen" 0.7 s, slightly faster than Deepgram's interim text at 0.8 s), but the latest words can still change until the commit.
  - The app should display partials and replace them on commit. A manual `commit:true` on short pauses is an option to shorten the open tail, but ElevenLabs warns that frequent commits reduce accuracy.
- **`no_verbatim=true`** removes fillers and false starts: fewer extra words, a few more missing ones. Net gain is about 1.4 pt vs Whisper.
- **Keyterms** (`live-captions/keyterms.txt`, max 50 terms × 20 chars) give a gain of about 0.4-0.8 pt, at the edge of the noise.
- **Parakeet v3** (tested on user request after the decision; `harness/vad_offline_run.py`): the worst result here.
  - It keeps "uh" fillers, mishears names and project terms, and drops words at segment edges.
  - It's fast on CPU, but too inaccurate for this meeting to be useful, live or after the meeting. It's no rival to ElevenLabs batch Scribe v3 (8.3%).
  - The 6.5-min numbers are trimmed by time (segment start < 384.5 s) because `versus_oss.py`'s END phrase isn't in v3's text: `runs/pk3_file_65.txt`.
- **Parakeet unified**: the most accurate local model tried, but impractical here.
  - The int8 sherpa-onnx export ran 3.3× slower than real time on CPU, and 3.2× with the sherpa-onnx CUDA wheel, because int8 ops fall back to CPU.
  - NeMo on Windows segfaulted while loading the fp32 model.
  - Possible future uses: an after-meeting whole-file transcript, where speed doesn't matter, or a hosted or WSL2 GPU setup.
- **External benchmark**: [benchmarks.coval.ai/stt](https://benchmarks.coval.ai/stt) (short voice-agent clips, ~900 samples).
  - ElevenLabs Scribe v3 Live 2.2% WER; Deepgram Nova-3 4.3%; Soniox RT v6 2.7%; Gemini 3.5 Transcribe Live 2.0%; AssemblyAI Universal 3.6 Pro 1.2%; Qwen3-ASR 1.7B 2.1%.
  - The ordering agrees with ours for ElevenLabs vs Deepgram. Absolute numbers aren't comparable with ours (clean short clips vs an accented meeting).
- **Not tested** (stopped by the user): Nemotron streaming, Moonshine v2, Kyutai, Voxtral, WhisperLiveKit Whisper, Gemini 3.5 Transcribe Live, AssemblyAI, Deepgram nova-2-meeting.
  - Research notes: `research.md` (open models) and `research_parakeet.md`.

## Recommended live settings
`wss://api.elevenlabs.io/v1/speech-to-text/realtime?model_id=scribe_v3_live&audio_format=pcm_16000&language_code=en&commit_strategy=turn_prediction&no_verbatim=true&keyterms=...`
- Header `xi-api-key`; 16 kHz s16le mono in 100 ms chunks as `{"message_type":"input_audio_chunk","audio_base_64":...,"commit":false,"sample_rate":16000}`.
- To flush at the end, send an empty chunk with `commit:true`.
- Show `partial_transcript` as the live line and replace it with `committed_transcript`.
- Sessions close after 15 s without client messages, so keep streaming silence.
- Pricing: $0.22/h (+ $0.08/h for keyterms). The Starter plan includes 27 h/month.
- The API key is in `oss_live/elevenlabs/.env`. Rotate it: it was pasted in chat.

## Reproduce (from the project folder)
```
cd oss_live
uv venv --python 3.12 elevenlabs/.venv && uv pip install --python elevenlabs/.venv/Scripts/python.exe websockets numpy
elevenlabs/.venv/Scripts/python.exe -I harness/elevenlabs_run.py el_v3_kt_nv --keyterms ../live-captions/keyterms.txt --no-verbatim
#   other runs: --model scribe_v2_realtime --commit vad | --commit vad --silence 0.5 | (no --keyterms) | (no --no-verbatim)
cd ..
python compare.py whisper_large_v3.txt oss_live/runs/el_v3_kt_nv.txt el      # also with dg_en_kt.txt
python oss_live/versus_oss.py "EL=oss_live/runs/el_v3_kt_nv.txt"
python oss_live/harness/latency.py oss_live/runs/el_v3_kt_nv.jsonl
```
Run outputs: `oss_live/runs/*.jsonl|.txt|.stats.json`. Deepgram live baselines: `runs/dg_live_ep300.jsonl` / `dg_live_ep800.jsonl`, rebuilt from `raw_ep*.jsonl`.
Harness: `harness/sim.py` (simulated live clock, VRAM monitor) and `harness/sherpa_run.py`, `harness/nemo_run.py`, `harness/wlk_run.py`; their environments were deleted.

## After-meeting batch test (2026-11-16 evening)
ElevenLabs batch **Scribe v3** on `meeting.ogg` (`model_id=scribe_v3, language_code=en, diarize=true, no_verbatim=true, timestamps_granularity=word` + the 12 keyterms). Script: `harness/el_batch.sh`, text/turns: `harness/el_batch_text.py`. Took 11.5 s for 7.7 min. Cost ≈ $0.03.

| System | vs Whisper, 7.7 min | vs Whisper, 6.5 min | vs Wispr live | vs Wispr refined |
|---|---|---|---|---|
| **ElevenLabs batch Scribe v3** | **8.3%** | 7.1% | 7.6% | 6.2% |
| Deepgram whole file (same run of compare.py) | 9.5% | 8.5% | 7.5% | 8.7% |
| Wispr refined | — | 7.2% | 6.4% | 0 |

Diarization found 4 speakers (Wispr's refined transcript also had 4). Outputs: `runs/el_batch_v3.json|.txt`, `runs/el_batch_v3_turns.txt`. Conclusion: ElevenLabs batch is about Wispr-refined quality and beats Deepgram's file transcript by ~1 pt.
