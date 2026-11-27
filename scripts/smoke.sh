#!/bin/bash -e
# Smoke test for the kit, after you change it: records tests/smoke (a static page, no app) with the
# engine's Playwright and the profile this folder would get (DEMO_PROFILE overrides). Runs from any
# folder. Writes walkie.mp4 (built from its layers) and contact.png in $WALKIE_SMOKE_DIR, and fails on a FLASH.
S="$(cd "$(dirname "$0")" && pwd)"
ROOT="$S/.."
WT="$ROOT/bin/wt"
D="${WALKIE_SMOKE_DIR:-${TMPDIR:-/tmp}/walkie-smoke}"
rm -rf "$D" && mkdir -p "$D"
cp "$ROOT/tests/smoke/"* "$D/"
cp "$ROOT/profiles/default/fonts/"Prompt-*.woff2 "$D/"  # the test pages are set in Prompt
"$WT" profile "$D"
Q() { node --no-warnings "$S/queue.ts" "smoke $1" -- "${@:2}"; }  # every heavy step waits its turn
Q tts "$WT" tts "$D" < "$D/narration.tsv" 2>/dev/null
DEMO_DIR="$D" Q record "$ROOT/node_modules/.bin/playwright" test -c "$S/demo.config.ts"
R=$(command ls -d "$D"/results/*/ | head -1)
Q build node --no-warnings "$S/build.ts" "$D" "$R/video.webm"
Q check "$WT" frames "$D/walkie.mp4" "$D" "$D/contact.png" | tee "$D/frames.log"
echo "video: $D/walkie.mp4"
! grep -q FLASH "$D/frames.log"
