# Walkie engine

Walkie makes **walkies**: slow, narrated videos of one code change, for people outside engineering (support, operations, organizers). This repo is the engine: it records the app, speaks the narration and builds the MP4. An agent drives it through the [walkie plugin](https://github.com/nijicha/skills/tree/main/plugins/walkie) for Claude Code.

- Playwright records a web app; a Flutter app records on an iOS Simulator or Android emulator.
- A voice explains each step, on your Mac's CPU: Thai (MakHam) or English (Kokoro), set by the profile.
- Captions, a highlight box, a cursor and cards come from a per-team profile.
- A video is kept as layers (page, overlay, text, voice), so narration, text or the look can change without recording again.

Docs: [nijicha.github.io/walkie-engine](https://nijicha.github.io/walkie-engine)

## Install

macOS on Apple Silicon only for now (ONNX Runtime, which runs the voice, ships no prebuilt library for Intel Macs).

```bash
brew install nijicha/tap/walkie
walkie setup        # once: Chromium, then pick the voices (Thai, English)
```

Then, in Claude Code:

```
/plugin install walkie --marketplace nijicha/skills
```

## Command line

| Command | Does |
|---|---|
| `walkie setup` | Downloads Chromium, asks which voices to set up and downloads them; checks ffmpeg |
| `walkie setup th en` | The same, for the voices named: `th` Thai (MakHam), `en` English (Kokoro) |
| `walkie setup list` | The voices: model, size, installed or not |
| `walkie say th\|en out.wav "text"` | Says one line with that language's default voice |
| `walkie mcp` | Runs the MCP server on stdio (the plugin starts it) |
| `walkie smoke` | Records a static test page, no app needed |
| `walkie preview <profile> [out.html]` | Shows every state of a profile's look on a sample page |
| `walkie profile --list` / `--resolve <name>` | Lists profiles / prints one merged over `default` |
| `walkie version`, `walkie path` | The engine version / its folder |
| `walkie test` | The Rust tests, schema checks and the smoke recording |

## Layout

| Folder | What |
|---|---|
| `bin/walkie` | The command line |
| `mcp/` | MCP server: `start`, `storyboard`, `record`, `revoice`, `rerender`, … |
| `scripts/` | Recording kit (Playwright), overlay drawing, build, queue, library (TypeScript) |
| `wt/` | The engine's internal tools, called by `walkie` and the MCP server: TTS, video mixing, contact sheet, simulator recording (Rust) |
| `flutter/` | The recording kit for Flutter apps (Dart) |
| `profiles/default/` | The default look and voice; every team profile is merged over it |
| `schema/` | JSON Schemas for profiles, videos and events: the contract for apps built on the engine |
| `docs/` | The docs site |

Data lives in `~/Library/Application Support/walkie` (videos and the queue), `~/.config/walkie/profiles` (team profiles) and `~/.cache/walkie` (voice model). `WALKIE_DATA`, `WALKIE_HOME` and `WALKIE_PROFILES` move them.

## Develop

Needs Node 23.6+, pnpm, ffmpeg and [mise](https://mise.jdx.dev) (it provides the Rust that builds `wt`).

```bash
bin/walkie setup     # Node packages, Chromium, builds bin/wt, voice model
bin/walkie test
pnpm typecheck
pnpm test:mcp        # every MCP tool, end to end
```

## Release

Bump the version in `package.json`, add a `## <version>` section to `CHANGELOG.md`, then push a tag that equals it (`git tag -a v0.1.0-alpha -m … && git push origin v0.1.0-alpha`). The **Release** workflow builds the archive with `scripts/package.sh` and publishes a GitHub Release; then update `url` and `sha256` in `Formula/walkie.rb` in [nijicha/homebrew-tap](https://github.com/nijicha/homebrew-tap).

## License

MIT, see [LICENSE](LICENSE). Bundled fonts keep their own licenses: Prompt and CaskaydiaCove Nerd Font are under the SIL Open Font License (`OFL.txt` next to each). The voice models, MakHam (Thai) and Kokoro-82M (English), are Apache-2.0 and are downloaded at setup, not bundled. The English G2P, misaki-rs, is MIT and is built without espeak-ng.
