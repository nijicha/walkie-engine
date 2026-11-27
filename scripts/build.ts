// Builds walkie.mp4 from its layers. Used by the MCP server (record, rerender) and smoke.sh.
//
// Usage: node scripts/build.ts DIR [RAW.webm]
//   with RAW (a fresh recording): cut it into the page layer (content.mp4) and take backdrops first
//   without RAW: reuse DIR/content.mp4, e.g. after a theme or text change
// Then: overlay layer (render.ts, from DIR/events.json and DIR/profile.json) -> DIR/overlay/, and
// compose page + overlay + voice -> DIR/walkie.mp4. The overlay frames are removed after.
import { execFileSync } from "node:child_process"
import { rmSync } from "node:fs"
import { join } from "node:path"
import { WT } from "./paths.ts"
import { renderOverlay } from "./render.ts"

const mix = (...args: string[]) => execFileSync(WT, ["mix", ...args], { stdio: ["ignore", "inherit", "inherit"] })

export async function build(dir: string, raw?: string) {
  if (raw) {
    mix("content", raw, dir)
    mix("backdrops", dir)
  }
  const content = join(dir, "content.mp4")
  const seconds = parseFloat(execFileSync("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", content], { encoding: "utf8" }))
  const overlay = join(dir, "overlay")
  const r = await renderOverlay(dir, overlay, seconds)
  console.log(`overlay: ${r.frames} frames captured for ${r.total} video frames`)
  const out = join(dir, "walkie.mp4")
  mix("compose", dir, out)
  rmSync(overlay, { recursive: true, force: true })
  return { video: out, seconds }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [dir, raw] = process.argv.slice(2)
  if (!dir) {
    console.error("usage: node scripts/build.ts DIR [RAW.webm]")
    process.exit(2)
  }
  await build(dir, raw)
}
