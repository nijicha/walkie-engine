# Changelog

## 0.1.1-alpha

- `start` takes `min_engine`, the oldest engine the calling skill works with, and refuses an older engine with the fix (`brew upgrade walkie`). It also reports the engine version.
- The PR text ends with a "Made with Walkie" line.
- Fixed: for a Flutter app, `start` pointed at a `references/flutter.md` next to the engine, from the old layout. It now names the walkie skill's reference.

## 0.1.0-alpha

First release as **walkie**, split out of the private `walkthrough` repo with a fresh history.

- Renamed walkthrough to walkie: the `walkie` command, `walkie:*` MCP tools, `WALKIE_*` environment variables.
- The engine has its own data folders: `~/Library/Application Support/walkie`, `~/.config/walkie/profiles`, `~/.cache/walkie`. They no longer use the Mac app's folder. Old `walkthrough` folders are not migrated.
- New commands: `walkie version` and `walkie path`.
- Installs with Homebrew: `brew install nijicha/tap/walkie`. Apple Silicon only.
- The skill, subagent and MCP registration moved to the `walkie` plugin in [nijicha/skills](https://github.com/nijicha/skills).
