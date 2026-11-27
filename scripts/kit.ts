// Everything a walkie spec imports, from the engine's own Playwright. A spec imports from
// "./kit" (written by `start` next to it), never from "@playwright/test": the repo may have its own
// Playwright, and two copies in one run fail.
export { expect, test } from "@playwright/test"
export type { APIRequestContext, Locator, Page } from "@playwright/test"
export { Demo, type HighlightOptions } from "./demo.ts"
