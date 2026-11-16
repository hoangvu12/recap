#!/bin/bash
# ElevenLabs batch Scribe v3 on meeting.ogg with diarization + keyterms. Usage: bash el_batch.sh <out.json>
cd "$(dirname "$0")/../.." || exit 1
KEY=$(grep -E '^(ELEVENLABS_API_KEY|XI_API_KEY)=' oss_live/elevenlabs/.env | head -1 | cut -d= -f2- | tr -d '\r"')
[ -z "$KEY" ] && { echo "no key found in oss_live/elevenlabs/.env"; exit 1; }
args=()
while IFS= read -r t || [ -n "$t" ]; do t=${t%$'\r'}; [ -n "$t" ] && [ "${t:0:1}" != "#" ] && args+=(-F "keyterms=$t"); done < live-captions/keyterms.txt
curl -s -X POST https://api.elevenlabs.io/v1/speech-to-text \
  -H "xi-api-key: $KEY" \
  -F model_id=scribe_v3 -F language_code=en -F diarize=true -F no_verbatim=true \
  -F timestamps_granularity=word "${args[@]}" \
  -F file=@meeting.ogg -o "$1" -w "%{http_code} %{time_total}s\n"
