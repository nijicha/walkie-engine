#!/bin/sh
# Builds dist/walkie-<version>.tar.gz for Homebrew: the engine, its Node packages (production only,
# plain node_modules) and a release build of bin/wt. Apple Silicon only: ONNX Runtime ships no
# prebuilt library for Intel Macs. Needs cargo (from mise), pnpm and node.
#
# Usage: scripts/package.sh        prints the archive path and its sha256
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
[ "$(uname -m)" = arm64 ] || { echo "build on an Apple Silicon Mac" >&2; exit 1; }
V=$(node -p "require('./package.json').version")
OUT="dist/walkie-$V"
rm -rf dist
mkdir -p "$OUT/bin"

(cd wt && cargo build --release --locked --quiet)
cp wt/target/release/wt bin/walkie "$OUT/bin/"
cp -R mcp scripts flutter fonts profiles schema tests "$OUT/"
cp package.json pnpm-lock.yaml tsconfig.json LICENSE README.md CHANGELOG.md "$OUT/"
(cd "$OUT" && pnpm install --prod --frozen-lockfile --ignore-scripts --config.node-linker=hoisted --silent)

tar -C dist -czf "dist/walkie-$V.tar.gz" "walkie-$V"
echo "dist/walkie-$V.tar.gz"
shasum -a 256 "dist/walkie-$V.tar.gz" | cut -d' ' -f1
