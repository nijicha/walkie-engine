# Default profile notes

The Walkie look, the same as the Mac app: a technical document. Paper cards with a tag band in capitals under a black rule and a large left-aligned title, a black square caption bar, one bold orange for the highlight box, the step badge and bold words. Prompt for all overlay text (captions, cards, tag band). Set `layout.shape` to `round` for rounded corners. Use it for a repo without its own profile, or copy this folder to start one (see `references/setup-interview.md`).

Before the intro, log in and seed the data the demo needs: `wt mix` cuts everything before `demo.intro()`. Set `record.base_url` in `profile.json` to where the app runs. Put login and seed-data code in the profile's `helpers.ts`: the spec cannot import the repo's Playwright page objects.
