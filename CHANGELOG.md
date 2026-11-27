# Changelog

## 0.2.0-alpha

- English narration. A profile with `"language": "en"` in its `voice.json` is voiced by Kokoro-82M (US English, int8, on the CPU) and starts from the new `profiles/default/voice.en.json`. Pick a speaker with `"speaker"`: `af_heart` (default), `af_bella`, `af_nicole`, `af_sarah`, `am_michael`, `am_fenrir` or `am_puck`.
- The English G2P is misaki-rs, built without espeak-ng. A word it cannot read gets an `UNKNOWN` flag in the storyboard table and is left out until it has a lexicon entry (`"Eventpop": "ˈɛvəntpˌɑp"`) or inline `{phonemes}`.
- Versions (0.1.1, v2.3), times (10:30) and dates (28 Nov, November 28) are rewritten before the English G2P so they are read as people say them.
- `start` says which language to write the narration in. Tool descriptions no longer assume Thai.
- `wt tts th|en --setup` and `wt tts th|en OUT.wav "text"` (one line with the default voice). `walkie setup en` also downloads the English model (92 MB); otherwise it downloads on first use.

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
