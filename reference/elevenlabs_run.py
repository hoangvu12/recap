"""Live replay of meeting16k.wav through ElevenLabs realtime speech-to-text (Scribe).

Usage: python elevenlabs_run.py <label> [--model scribe_v3_live] [--commit turn_prediction|vad|manual]
       [--silence 0.6] [--lang en] [--keyterms FILE] [--seconds N] [--chunk 0.1]
Sends 16 kHz PCM in --chunk sized pieces at real-time pace; every partial_transcript is
logged as a partial and every committed_transcript as a final, stamped with the audio time
(seconds since the first chunk was sent). Writes ../runs/<label>.jsonl/.txt/.stats.json.
"""
import argparse
import asyncio
import base64
import json
import os
import pathlib
import sys
import time
import urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import websockets  # noqa: E402

from sim import OSS, load_audio  # noqa: E402


def key():
    for line in open(OSS / "elevenlabs" / ".env", encoding="utf-8"):
        if line.startswith("ELEVENLABS_API_KEY="):
            return line.split("=", 1)[1].strip()


async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("label")
    ap.add_argument("--model", default="scribe_v3_live")
    ap.add_argument("--commit", default="turn_prediction")
    ap.add_argument("--silence", type=float, default=0)
    ap.add_argument("--lang", default="en")
    ap.add_argument("--keyterms", default="")
    ap.add_argument("--seconds", type=float, default=0)
    ap.add_argument("--chunk", type=float, default=0.1)
    ap.add_argument("--no-verbatim", action="store_true", help="drop fillers/false starts")
    a = ap.parse_args()

    x, sr = load_audio()
    if a.seconds:
        x = x[: int(a.seconds * sr)]
    pcm = (x * 32767).clip(-32768, 32767).astype("<i2").tobytes()
    q = {"model_id": a.model, "audio_format": "pcm_16000", "commit_strategy": a.commit}
    if a.lang:
        q["language_code"] = a.lang
    if a.no_verbatim:
        q["no_verbatim"] = "true"
    if a.silence:
        q["vad_silence_threshold_secs"] = a.silence
    qs = urllib.parse.urlencode(q)
    if a.keyterms:
        terms = [t.strip() for t in open(a.keyterms, encoding="utf-8") if t.strip()][:50]
        qs += "".join("&keyterms=" + urllib.parse.quote(t[:20]) for t in terms)
    url = "wss://api.elevenlabs.io/v1/speech-to-text/realtime?" + qs

    events, info = [], {}
    seg, last = 0, ""
    t0 = None
    step = int(a.chunk * sr) * 2
    async with websockets.connect(url, additional_headers={"xi-api-key": key()}, max_size=None) as ws:
        async def sender():
            nonlocal t0
            t0 = time.perf_counter()
            for i, off in enumerate(range(0, len(pcm), step)):
                delay = t0 + i * a.chunk - time.perf_counter()
                if delay > 0:
                    await asyncio.sleep(delay)
                await ws.send(json.dumps({"message_type": "input_audio_chunk", "sample_rate": 16000, "commit": False,
                                          "audio_base_64": base64.b64encode(pcm[off:off + step]).decode()}))
            await asyncio.sleep(0.5)
            await ws.send(json.dumps({"message_type": "input_audio_chunk", "audio_base_64": "", "commit": True,
                                      "sample_rate": 16000}))

        async def receiver():
            nonlocal seg, last
            async for raw in ws:
                m = json.loads(raw)
                t = round(time.perf_counter() - (t0 or time.perf_counter()), 3)
                kind = m.get("message_type")
                if kind == "session_started":
                    info["config"] = m.get("config")
                elif kind == "partial_transcript":
                    txt = (m.get("text") or "").strip()
                    if txt and txt != last:
                        events.append({"t": t, "kind": "partial", "seg": seg, "text": txt})
                        last = txt
                elif kind == "committed_transcript":
                    txt = (m.get("text") or "").strip()
                    if txt:
                        events.append({"t": t, "kind": "final", "seg": seg, "text": txt})
                    seg, last = seg + 1, ""
                    if t > len(x) / sr + 0.4 and send_task.done():
                        return
                elif "error" in m:
                    print("SERVER:", m)
                    info.setdefault("errors", []).append(m)
                    if kind in ("auth_error", "quota_exceeded", "invalid_request", "unaccepted_terms"):
                        return

        send_task = asyncio.create_task(sender())
        try:
            await asyncio.wait_for(receiver(), timeout=len(x) / sr + 30)
        except asyncio.TimeoutError:
            print("timeout waiting for final commit")
        send_task.cancel()

    out = OSS / "runs" / a.label
    with open(f"{out}.jsonl", "w", encoding="utf-8") as f:
        for e in events:
            f.write(json.dumps(e, ensure_ascii=False) + "\n")
    with open(f"{out}.txt", "w", encoding="utf-8") as f:
        for e in events:
            if e["kind"] == "final":
                mm, ss = divmod(int(e["t"]), 60)
                f.write(f"[{mm:02d}:{ss:02d}] {e['text']}\n")
    stats = {"label": a.label, "settings": vars(a), "audio_s": round(len(x) / sr, 2),
             "finals": sum(e["kind"] == "final" for e in events), **info}
    pathlib.Path(f"{out}.stats.json").write_text(json.dumps(stats, indent=1), encoding="utf-8")
    print(json.dumps({k: v for k, v in stats.items() if k != "config"}))


if __name__ == "__main__":
    asyncio.run(main())
