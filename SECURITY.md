# Security

## Supported versions

walkie is in alpha. Only the latest release gets fixes: `brew upgrade walkie` to get it.

## Reporting a vulnerability

Report it privately with GitHub's [Report a vulnerability](https://github.com/nijicha/walkie-engine/security/advisories/new) form, not in a public issue. Say what an attacker could do, and how to make it happen. You will get an answer within a week; this is a one-person project, so a fix may take longer.

## What walkie does on your Mac

Useful for judging what counts as a vulnerability:

- It runs locally. The MCP server talks to Claude Code over stdio only; it opens no network port.
- It records the app you point it at, with its own Playwright Chromium, and keeps the videos in `~/Library/Application Support/walkie`. Nothing is uploaded.
- It downloads voice models and data from Hugging Face and GitHub at setup or on first use, each pinned to a revision and checked against a SHA-256 before use.
- A profile's `NOTES.md` and spec helpers are read by the agent and run by Playwright: treat a profile from someone else like code you run.
