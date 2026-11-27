# Walkie engine

Walkie makes **walkies**: slow, narrated videos of one code change, for people outside engineering (support, operations, organizers). This repo is the engine: it records the app, speaks the narration and builds the MP4. An agent drives it through the [walkie plugin](https://github.com/nijicha/skills/tree/main/plugins/walkie) for Claude Code.

- Playwright records a web app; a Flutter app records on an iOS Simulator or Android emulator.
- A Thai voice explains each step (MakHam, on your Mac's CPU). More languages are planned.
- Captions, a highlight box, a cursor and cards come from a per-team profile.
- A video is kept as layers (page, overlay, text, voice), so narration, text or the look can change without recording again.

Docs: [nijicha.github.io/walkie-engine](https://nijicha.github.io/walkie-engine)

## Install

macOS only for now.

```bash
brew install nijicha/tap/walkie
walkie setup        # once: Chromium and the Thai voice model
```

Then, in Claude Code:

```
/plugin install walkie --marketplace nijicha/skills
```

## Command line

| Command | Does |
|---|---|
| `walkie setup` | Downloads Chromium and the voice model; checks ffmpeg |
| `walkie mcp` | Runs the MCP server on stdio (the plugin starts it) |
| `walkie smoke` | Records a static test page, no app needed |
| `walkie preview <profile> [out.html]` | Shows every state of a profile's look on a sample page |
| `walkie profile --list` / `--resolve <name>` | Lists profiles / prints one merged over `default` |
| `walkie version`, `walkie path` | The engine version / its folder |
| `walkie test` | `wt`'s tests, schema checks and the smoke recording |

## Layout

| Folder | What |
|---|---|
| `bin/walkie` | The command line |
| `mcp/` | MCP server: `start`, `storyboard`, `record`, `revoice`, `rerender`, … |
| `scripts/` | Recording kit (Playwright), overlay drawing, build, queue, library (TypeScript) |
| `wt/` | `wt`: Thai TTS, video mixing, contact sheet, simulator recording (Rust) |
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

## License

MIT, see [LICENSE](LICENSE). Bundled fonts keep their own licenses: Prompt and CaskaydiaCove Nerd Font are under the SIL Open Font License (`OFL.txt` next to each). The Thai voice model (MakHam) is Apache-2.0 and is downloaded at setup, not bundled.
