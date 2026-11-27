#!/bin/bash
# Sound for the docs videos, from scenes.json. No voice: sound effects and a music bed.
#
#   sound.sh gen     generate every sound and both music beds with ElevenLabs
#                    (needs ELEVENLABS_API_KEY; files go to docs/videos/src/sfx/, kept out of git)
#   sound.sh mix     put the sounds on each video's timeline and write
#                    docs/videos/<name>.mp4 with audio (the silent cut is kept as <name>-silent.mp4)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
videos=$(dirname "$here")
script="$here/scenes.json"
sfx="$here/sfx"
mkdir -p "$sfx"

gen() {
  : "${ELEVENLABS_API_KEY:?set ELEVENLABS_API_KEY}"
  for name in $(jq -r '.sounds | keys[]' "$script"); do
    out="$sfx/$name.mp3"
    [ -s "$out" ] && { echo "have $name"; continue; }
    body=$(jq -c --arg n "$name" '.sounds[$n] | {
      text: .prompt, duration_seconds: .seconds, prompt_influence: 0.6,
      loop: (.loop // false), model_id: "eleven_text_to_sound_v2" }' "$script")
    echo "sound $name"
    curl -sf https://api.elevenlabs.io/v1/sound-generation \
      -H "xi-api-key: $ELEVENLABS_API_KEY" -H "content-type: application/json" \
      -d "$body" -o "$out"
  done
  for video in $(jq -r '.videos | keys[]' "$script"); do
    out="$sfx/music-$video.mp3"
    [ -s "$out" ] && { echo "have music-$video"; continue; }
    body=$(jq -c --arg v "$video" '{ prompt: .music.prompt,
      music_length_ms: ((.videos[$v].length + 2) * 1000 | floor) }' "$script")
    echo "music $video"
    curl -sf https://api.elevenlabs.io/v1/music \
      -H "xi-api-key: $ELEVENLABS_API_KEY" -H "content-type: application/json" \
      -d "$body" -o "$out"
  done
}

mix() {
  for video in $(jq -r '.videos | keys[]' "$script"); do
    file=$(jq -r --arg v "$video" '.videos[$v].file' "$script")
    length=$(jq -r --arg v "$video" '.videos[$v].length' "$script")
    silent="$videos/${file%.mp4}-silent.mp4"
    [ -f "$silent" ] || mv "$videos/$file" "$silent"

    args=(-i "$silent" -i "$sfx/music-$video.mp3")
    vol=$(jq -r '.music.volume' "$script")
    fade=$(awk "BEGIN{printf \"%.3f\", $length - 3}")
    graph="[1:a]atrim=0:$length,volume=$vol,afade=t=in:d=1.5,afade=t=out:st=$fade:d=3[m];"
    labels="[m]"
    i=2
    while IFS=$'\t' read -r sound at dur v; do
      args+=(-stream_loop -1 -i "$sfx/$sound.mp3")
      ms=$(awk "BEGIN{printf \"%d\", $at * 1000}")
      if [ "$dur" = "null" ]; then
        dur=$(jq -r --arg s "$sound" '.sounds[$s].seconds' "$script")
      fi
      out=$(awk "BEGIN{printf \"%.3f\", $dur - 0.15}")
      graph+="[$i:a]atrim=0:$dur,afade=t=out:st=$out:d=0.15,volume=$v,adelay=$ms|$ms[c$i];"
      labels+="[c$i]"
      i=$((i + 1))
    done < <(jq -r --arg v "$video" '.videos[$v].cues[] | [.sound, .at, (.for // "null"), (.volume // 0.8)] | @tsv' "$script")
    n=$((i - 1))
    graph+="${labels}amix=inputs=$n:normalize=0:duration=first,atrim=0:$length,alimiter=limit=0.9[a]"

    echo "mix $video ($((n - 1)) cues)"
    ffmpeg -v error -y "${args[@]}" -filter_complex "$graph" \
      -map 0:v -map "[a]" -c:v copy -c:a aac -b:a 160k -shortest -movflags +faststart "$videos/$file"
  done
}

case "${1:-}" in
  gen) gen ;;
  mix) mix ;;
  *) sed -n '2,8p' "$0"; exit 1 ;;
esac
