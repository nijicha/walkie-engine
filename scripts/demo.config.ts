import { defineConfig } from "@playwright/test"
import { readFileSync } from "fs"
import { resolve } from "path"

// Records one walkie with the engine's own Playwright and Chromium.
// DEMO_DIR is the walkie folder holding demo.spec.ts and profile.json.
// baseURL: WALKIE_BASE_URL, else record.base_url in the profile.
const demoDir = resolve(process.env.DEMO_DIR ?? ".")
const profile = JSON.parse(readFileSync(resolve(demoDir, "profile.json"), "utf8"))
const { video, zoom } = profile.theme.layout

// The page lays out at video / zoom CSS pixels, like a browser at 150 %, and is recorded at that
// size. wt mix scales it up to the video size. (Playwright records CSS pixels whatever the
// deviceScaleFactor, so a larger device scale would not make the recording sharper.)
const viewport = { width: Math.round(video[0] / zoom), height: Math.round(video[1] / zoom) }

export default defineConfig({
  testDir: demoDir,
  testMatch: "demo.spec.ts",
  retries: 0,
  workers: 1,
  timeout: 5 * 60 * 1000,
  expect: { timeout: 10_000 },
  outputDir: resolve(demoDir, "results"),
  reporter: [["list"]],
  projects: [
    {
      name: "demo",
      use: {
        browserName: "chromium",
        channel: "chromium", // full Chromium in new headless mode, as close to a real browser as it gets
        baseURL: process.env.WALKIE_BASE_URL ?? profile.profile.record?.base_url ?? undefined,
        viewport,
        video: { mode: "on", size: viewport },
        trace: "on",
        actionTimeout: 15_000,
        launchOptions: { slowMo: 150 },
      },
    },
  ],
})
