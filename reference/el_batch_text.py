"""Turn an ElevenLabs batch JSON into plain text (for compare.py) and a speaker-turn view.
Usage: python el_batch_text.py in.json out.txt [turns.txt]"""
import json, sys
from collections import Counter

d = json.load(open(sys.argv[1], encoding="utf-8"))
open(sys.argv[2], "w", encoding="utf-8").write(d["text"].strip() + "\n")
words = [w for w in d.get("words", []) if w.get("type") == "word"]
spk = Counter(w.get("speaker_id") for w in words)
print("speakers:", dict(spk))
if len(sys.argv) > 3:
    turns, cur, buf, t0 = [], None, [], 0.0
    for w in d.get("words", []):
        s = w.get("speaker_id")
        if w.get("type") == "word" and s != cur:
            if buf:
                turns.append(f"[{t0:6.1f}] {cur}: {''.join(buf).strip()}")
            cur, buf, t0 = s, [], w["start"]
        buf.append(w["text"])
    if buf:
        turns.append(f"[{t0:6.1f}] {cur}: {''.join(buf).strip()}")
    open(sys.argv[3], "w", encoding="utf-8").write("\n".join(turns) + "\n")
    print("turns:", len(turns))
