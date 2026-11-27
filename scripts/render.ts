// Overlay layer: draws a recording's events (events.json) with a profile's theme as transparent
// PNG frames at the final video size, for wt mix compose to lay over the page layer (content.mp4).
//
// Usage: node scripts/render.ts DIR OUT_DIR SECONDS
//        DIR holds profile.json and events.json; OUT_DIR gets 000000.png ... at 30 fps.
//
// Time is exact, not real time: each frame applies the events up to its time, then sets every CSS
// animation (fades, the box gliding, the veil) to how far it has run. A frame is captured only when
// something changed; an unchanged frame is a hard link to the one before (no disk, no capture).
import { chromium } from "@playwright/test"
import { linkSync, mkdirSync, readFileSync, rmSync } from "node:fs"
import { extname, join } from "node:path"
import { cardAssets } from "./assets.ts"
import { overlayScript, VEIL, type Profile } from "./overlay.ts"

const FPS = 30

const fontData = (path: string) =>
  `url(data:font/${extname(path).slice(1)};base64,${readFileSync(path).toString("base64")})`

export async function renderOverlay(dir: string, out: string, seconds: number) {
  const profile: Profile = JSON.parse(readFileSync(join(dir, "profile.json"), "utf8"))
  const rec = JSON.parse(readFileSync(join(dir, "events.json"), "utf8"))
  const [W, H] = profile.theme.layout.video
  // Events are in px of the recorded page (video / zoom); the overlay draws at video px. A device
  // recording fills only the phone's place on the stage (wt mix wrote stage.json).
  // Where the phone sits is part of the page layer (stage.json); how it is framed comes from the
  // profile, so a rerender can restyle it.
  const dev = profile.theme.layout.device ?? {}
  const stage = rec.source === "device"
    ? { ...JSON.parse(readFileSync(join(dir, "stage.json"), "utf8")),
        ...Object.fromEntries(["radius", "bezel", "caption_size"].filter((k) => k in dev).map((k) => [k, dev[k]])) }
    : null
  const area = stage?.phone ?? { x: 0, y: 0, w: W, h: H }
  const sx = area.w / rec.viewport.width
  const sy = area.h / rec.viewport.height
  const scale = (e: any) => {
    if (e.type === "camera") return e // the page layer zoomed already
    const r = e.rect && { x: area.x + e.rect.x * sx, y: area.y + e.rect.y * sy, w: e.rect.w * sx, h: e.rect.h * sy }
    if (e.type === "cursor") return { ...e, x: area.x + e.x * sx, y: area.y + e.y * sy }
    return r ? { ...e, rect: r } : e
  }
  const events = rec.events.map(scale)
  const theme = { ...profile.theme, layout: { ...profile.theme.layout, zoom: 1 } }
  const f = profile.theme.fonts

  rmSync(out, { recursive: true, force: true })
  mkdirSync(out, { recursive: true })
  const browser = await chromium.launch({ channel: "chromium" })
  try {
    const page = await browser.newPage({ viewport: { width: W, height: H } })
    await page.setContent(`<!doctype html><html style="background: transparent"><head><meta charset="utf-8"></head>
      <body style="margin: 0; background: transparent"></body></html>`)
    await page.evaluate(overlayScript, {
      t: theme,
      fonts: { medium: fontData(f.sans.medium), bold: fontData(f.sans.bold), monoRegular: fontData(f.mono.regular), monoBold: fontData(f.mono.bold) },
      assets: cardAssets(profile.theme),
      VEIL,
      stage,
    })
    await page.evaluate(() => document.fonts.ready)
    await page.evaluate((events) => {
      let next = 0
      let running: { a: Animation; start: number }[] = []
      const seen = new WeakSet<Animation>()
      ;(window as any).__demoFrame = (t: number) => {
        let changed = false
        while (next < events.length && events[next].t <= t) {
          const e = events[next++]
          ;(window as any).__demoApply(e)
          for (const a of document.getAnimations()) { // also flushes styles, which starts transitions
            if (seen.has(a)) continue
            seen.add(a)
            a.pause()
            running.push({ a, start: e.t })
          }
          changed = true
        }
        running = running.filter(({ a, start }) => {
          if (a.playState === "idle") return false // replaced by a newer transition
          const end = Number(a.effect?.getComputedTiming().endTime ?? 0)
          const ms = Math.min((t - start) * 1000, end)
          a.currentTime = ms
          changed = true
          return ms < end
        })
        return changed
      }
    }, events)

    const total = Math.ceil(seconds * FPS)
    let shots = 0
    let last = ""
    for (let frame = 0; frame < total; frame++) {
      const file = join(out, `${String(frame).padStart(6, "0")}.png`)
      const changed = await page.evaluate((t) => (window as any).__demoFrame(t), frame / FPS)
      if (changed || !last) {
        await page.screenshot({ path: file, omitBackground: true })
        shots++
        last = file
      } else {
        linkSync(last, file)
      }
    }
    return { frames: shots, total, fps: FPS }
  } finally {
    await browser.close()
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [dir, out, seconds] = process.argv.slice(2)
  if (!dir || !out || !seconds) {
    console.error("usage: node scripts/render.ts DIR OUT_DIR SECONDS")
    process.exit(2)
  }
  const r = await renderOverlay(dir, out, parseFloat(seconds))
  console.log(`overlay: ${r.frames} frames captured for ${r.total} video frames -> ${out}`)
}
