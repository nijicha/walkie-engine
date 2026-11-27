# Changelog

## 0.1.0-alpha

First release as **walkie**, split out of the private `walkthrough` repo with a fresh history.

- Renamed walkthrough to walkie: the `walkie` command, `walkie:*` MCP tools, `WALKIE_*` environment variables.
- The engine has its own data folders: `~/Library/Application Support/walkie`, `~/.config/walkie/profiles`, `~/.cache/walkie`. They no longer use the Mac app's folder. Old `walkthrough` folders are not migrated.
- New commands: `walkie version` and `walkie path`.
- The skill, subagent and MCP registration moved to the `walkie` plugin in [nijicha/skills](https://github.com/nijicha/skills).
