# Contributing

Thanks for helping with walkie. Bug reports, pronunciation problems and ideas go in [issues](https://github.com/nijicha/walkie-engine/issues/new/choose); for a bug, paste the output of `walkie doctor`.

This repo is the engine. The Claude Code skill, subagent and MCP registration are in [nijicha/skills](https://github.com/nijicha/skills), and the Homebrew formula in [nijicha/homebrew-tap](https://github.com/nijicha/homebrew-tap).

## Set up a checkout

Needs an Apple Silicon Mac, Node 23.6+, pnpm, ffmpeg and [mise](https://mise.jdx.dev) (it provides the Rust that builds the engine's tools).

```bash
git clone https://github.com/nijicha/walkie-engine && cd walkie-engine
bin/walkie setup     # Node packages, Chromium, builds bin/wt, voices
bin/walkie doctor
```

To try your checkout from Claude Code, point the plugin's MCP server at it: run `bin/walkie mcp` from the checkout instead of the `walkie` on your PATH.

## Before a pull request

```bash
bin/walkie test      # Rust tests, schema checks, version order, smoke recording
pnpm typecheck
pnpm test:mcp        # every MCP tool end to end (records a short video)
```

- One change per pull request, with what it changes and how you tested it. A change to what a video looks or sounds like should say so, ideally with a contact sheet or a clip.
- Commit messages start with the area: `Docs: …`, `Voice: …`, `MCP: …`, or a plain sentence for engine-wide changes.
- A change users will notice gets a line in [CHANGELOG.md](CHANGELOG.md) under a new version.
- Schemas in `schema/` are the contract for apps built on the engine: add optional fields; do not rename or remove fields without a new `schema_version`.
- Docs (`docs/`) are plain HTML in English. No text smaller than 12px.

## Voices and models

A new voice or language must have a licence that allows redistribution and commercial use (Apache-2.0, MIT or similar; not non-commercial). Pin every downloaded file by revision and SHA-256 in `wt/src/tts/data.rs`, and add it to `walkie setup list`. Do not bundle GPL code such as espeak-ng.

## Releases

The maintainer merges pull requests and releases: bump `package.json` and the CHANGELOG, tag `vX.Y.Z` (the release workflow publishes the tarball and its sha256), then update the Homebrew formula.
