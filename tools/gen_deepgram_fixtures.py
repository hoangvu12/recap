"""Writes the Deepgram protocol fixtures for the adapter's tests.

There is no Deepgram key, so these are not recordings: they are Deepgram's
documented message shapes (Nova-3 live `Results` / `UtteranceEnd`, and the
pre-recorded response), filled with the real word timings ElevenLabs gave
for the test meeting (crates/providers/tests/fixtures/elevenlabs_batch.json,
the synthetic TTS meeting: Tom, Priya, David).

- deepgram_live.jsonl: the first two turns as a live session with
  `interim_results=true&endpointing=300&utterance_end_ms=1000`. Each line is
  {"t": seconds of audio sent before it, "msg": message}. Interim results
  every second of audio; a final (`is_final`) at each pause of 300 ms or more
  (`speech_final` true) or after 6 s of speech (`speech_final` false);
  `UtteranceEnd` 1 s after a turn's last word.
- deepgram_batch.json: the pre-recorded answer for all of it, diarized.
  Deepgram numbers speakers in no promised order, so the numbers are
  permuted (Tom = 1, Priya = 0, David = 2) to test the S1.. mapping.

Run: python tools/gen_deepgram_fixtures.py
"""

import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "crates" / "providers" / "tests" / "fixtures"
REQUEST = "5c1d0e7a-0000-4000-8000-000000000014"
MODEL = {"name": "general-nova-3", "version": "2025-04-17.21547", "arch": "nova-3"}
UUID = "40bd3654-e622-47c4-a111-63a61b23bfe8"


def dg_word(w, speaker=None):
    out = {
        "word": re.sub(r"[^\w'%]", "", w["text"]).lower(),
        "start": w["start"],
        "end": w["end"],
        "confidence": 0.99,
        "punctuated_word": w["text"],
    }
    if speaker is not None:
        out["speaker"] = speaker
        out["speaker_confidence"] = 0.9
    return out


def results(words, start, duration, is_final, speech_final):
    return {
        "type": "Results",
        "channel_index": [0, 1],
        "duration": round(duration, 3),
        "start": round(start, 3),
        "is_final": is_final,
        "speech_final": speech_final,
        "channel": {
            "alternatives": [
                {
                    "transcript": " ".join(w["punctuated_word"] for w in words),
                    "confidence": 0.99 if words else 0.0,
                    "words": words,
                }
            ]
        },
        "metadata": {"request_id": REQUEST, "model_info": MODEL, "model_uuid": UUID},
        "from_finalize": False,
    }


def main():
    source = json.loads((FIXTURES / "elevenlabs_batch.json").read_text(encoding="utf-8"))
    words = [w for w in source["words"] if w["type"] == "word"]

    # Batch: everything, diarized.
    speakers = {"speaker_0": 1, "speaker_1": 0, "speaker_2": 2}
    batch_words = [dg_word(w, speakers[w["speaker_id"]]) for w in words]
    batch = {
        "metadata": {
            "transaction_key": "deprecated",
            "request_id": REQUEST,
            "sha256": "0" * 64,
            "created": "2026-11-17T09:00:00.000Z",
            "duration": source["audio_duration_secs"],
            "channels": 1,
            "models": [UUID],
            "model_info": {UUID: MODEL},
        },
        "results": {
            "channels": [
                {
                    "alternatives": [
                        {
                            "transcript": " ".join(w["punctuated_word"] for w in batch_words),
                            "confidence": 0.99,
                            "words": batch_words,
                        }
                    ]
                }
            ]
        },
    }
    (FIXTURES / "deepgram_batch.json").write_text(json.dumps(batch, indent=1) + "\n", encoding="utf-8")

    # Live: the first two turns (Tom, then Priya).
    turns = []
    for w in words:
        if not turns or turns[-1][-1]["speaker_id"] != w["speaker_id"]:
            turns.append([])
        turns[-1].append(w)
    turns = turns[:2]

    lines = []  # (t, msg)
    window = 0.0  # the next Results window's start
    for turn in turns:
        # Finals: split at pauses >= 0.3 s, or after 6 s of speech.
        segments, current = [], []
        for w in turn:
            if current and (w["start"] - current[-1]["end"] >= 0.3):
                segments.append((current, True))
                current = []
            elif current and w["end"] - current[0]["start"] > 6.0:
                segments.append((current, False))
                current = []
            current.append(w)
        segments.append((current, True))

        for segment, speech_final in segments:
            final_at = segment[-1]["end"] + 0.3
            # Interims every second of audio while the segment is spoken.
            t = window + 1.0
            while t < final_at:
                heard = [dg_word(w) for w in segment if w["end"] <= t]
                if heard:
                    lines.append((t + 0.3, results(heard, window, t - window, False, False)))
                t += 1.0
            final = [dg_word(w) for w in segment]
            lines.append((final_at + 0.2, results(final, window, final_at - window, True, speech_final)))
            window = final_at
        last = turn[-1]["end"]
        lines.append((last + 1.0, {"type": "UtteranceEnd", "channel": [0, 1], "last_word_end": last}))

    lines.sort(key=lambda line: line[0])
    with open(FIXTURES / "deepgram_live.jsonl", "w", encoding="utf-8", newline="\n") as f:
        for t, msg in lines:
            f.write(json.dumps({"t": round(t, 3), "msg": msg}) + "\n")

    finals = [m for _, m in lines if m.get("is_final")]
    for m in finals:
        ws = m["channel"]["alternatives"][0]["words"]
        print(f'{m["speech_final"]!s:5} {ws[0]["start"]:6.2f}-{ws[-1]["end"]:6.2f} {m["channel"]["alternatives"][0]["transcript"]}')
    print(len(lines), "live messages,", sum(1 for _, m in lines if m.get("is_final") is False), "interim")


if __name__ == "__main__":
    main()
