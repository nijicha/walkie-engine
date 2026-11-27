import type { Locator, Page } from "@playwright/test"
import { readFileSync, writeFileSync } from "fs"
import { join } from "path"
import { CARD_KINDS, contentScript, VEIL, type Profile } from "./overlay.ts"

// Demo kit. A walkie is made of separate layers, so each can change without the others:
//
//   page     the app as Playwright records it, with no overlay on it (content.mp4)
//   overlay  caption bar, highlight box, spotlight, cursor, cards, navigation veil, and their motion:
//            not drawn while recording, but logged as timed events (events.json) that render.ts
//            draws later with the profile's theme
//   text     the captions and card HTML inside those events, each with an id, so it can be edited
//   voice    one clip per line (voice/), placed by the timeline (voice/timeline.json)
//
// The focus zoom belongs to the page layer (the browser draws the zoomed page sharply); the box is
// logged where the element sits after it. Every look comes from DEMO_DIR/profile.json.

const GAP = 500 // silence after each clip before the next one may start
const PAUSE = 2200
const MARKER_MS = 400 // how long the sync marker shows; wt mix cuts right after it

export function loadProfile(demoDir: string): Profile {
  return JSON.parse(readFileSync(join(demoDir, "profile.json"), "utf8"))
}

export type HighlightOptions = { pad?: number; spotlight?: boolean; zoom?: boolean | number }

type Rect = { x: number; y: number; w: number; h: number }
type Event = { ts: number; type: string; [key: string]: unknown }

export class Demo {
  private t0 = 0
  private deadline = 0
  private step = 0
  private zoomed = false
  private cardShown = false
  private boxShown = false
  private events: Event[] = []
  private counts: Record<string, number> = {}
  timeline: { name: string; start: number }[] = []
  private durations: Record<string, number>
  readonly voiceDir: string

  private page: Page
  private demoDir: string
  readonly profile: Profile

  private constructor(page: Page, demoDir: string, profile: Profile) {
    this.page = page
    this.demoDir = demoDir
    this.profile = profile
    this.voiceDir = join(demoDir, "voice")
    this.durations = JSON.parse(readFileSync(join(this.voiceDir, "durations.json"), "utf8"))
  }

  // Call once, after login and before the first page you want to show.
  /** demoDir defaults to DEMO_DIR, which demo.config.ts sets. (__dirname does not exist in an ES module repo.) */
  static async create(page: Page, demoDir = process.env.DEMO_DIR ?? ".") {
    const profile = loadProfile(demoDir)
    const demo = new Demo(page, demoDir, profile)
    // The page reports pointer moves and scrolls under the box (with its own timestamps).
    await page.exposeBinding("__demoEvent", (_source, e: Event) => void demo.events.push(e))
    const args = { hide: profile.theme.hide ?? [], zoomMs: profile.theme.layout.focus_zoom.ms }
    await page.addInitScript(contentScript, args)
    if (page.url() !== "about:blank") await page.evaluate(contentScript, args)
    return demo
  }

  private get layout() {
    return this.profile.theme.layout
  }

  private get seq() {
    return this.profile.sequence
  }

  private emit(e: { type: string; [key: string]: unknown }) {
    this.events.push({ ts: Date.now(), ...e })
  }

  // Stable ids for the text layer: caption-1, step-2, card-1 ...
  private id(kind: string) {
    this.counts[kind] = (this.counts[kind] ?? 0) + 1
    return `${kind}-${this.counts[kind]}`
  }

  // ---- timing -------------------------------------------------------------------------------

  // Time zero of the final video. Shows a small magenta square for MARKER_MS; wt mix cuts the
  // recording right after it, so the square never reaches the final video.
  async start() {
    await this.ready()
    await this.page.evaluate(() => {
      const m = document.createElement("div")
      m.id = "demo-marker"
      document.documentElement.appendChild(m)
    })
    await this.page.waitForTimeout(MARKER_MS)
    await this.page.evaluate(() => document.getElementById("demo-marker")?.remove())
    await this.page.evaluate(() => new Promise(requestAnimationFrame))
    this.t0 = Date.now()
  }

  // Start a narration clip, after the previous one has finished.
  async say(name: string) {
    if (!(name in this.durations)) throw new Error(`no voice clip ${name}: run wt tts`)
    await this.untilQuiet()
    const now = Date.now()
    this.timeline.push({ name, start: (now - this.t0) / 1000 })
    this.deadline = now + this.durations[name] * 1000 + GAP
  }

  async untilQuiet() {
    const left = this.deadline - Date.now()
    if (left > 0) await this.page.waitForTimeout(left)
  }

  pause(ms = PAUSE) {
    return this.page.waitForTimeout(ms)
  }

  // ---- sequence -----------------------------------------------------------------------------

  // Opening card. Shown at once (it is the first frame of the video), then fades out.
  // With intro off in sequence.json, only starts the clock and skips the clip.
  async intro(html: string, clip = "n0") {
    if (!this.seq.intro) return this.start()
    await this.showCard(html, { instant: true, kind: "intro" })
    await this.start()
    await this.pause(600)
    await this.say(clip)
    await this.untilQuiet()
    await this.hideCard()
  }

  // Short card between parts ("ก่อนแก้" / "หลังแก้"). Off unless sequence.json turns it on.
  async chapter(html: string, clip?: string) {
    if (!this.seq.chapter) return
    await this.clearStage()
    await this.showCard(html, { kind: "chapter" })
    if (clip) {
      await this.say(clip)
      await this.untilQuiet()
    } else {
      await this.pause(this.seq.chapter_ms)
    }
    await this.hideCard()
  }

  async summary(html: string, clip: string) {
    if (!this.seq.summary) return
    await this.clearStage()
    await this.showCard(html, { kind: "summary" })
    await this.pause(400)
    await this.say(clip)
    await this.untilQuiet()
    await this.pause(1500)
  }

  async outro(clip?: string) {
    if (!this.seq.outro || !this.seq.outro_html) return
    await this.hideCard()
    await this.showCard(this.seq.outro_html, { kind: "outro" })
    if (clip) {
      await this.say(clip)
      await this.untilQuiet()
    }
    await this.pause(1500)
  }

  // Call last: writes voice/timeline.json for the voice, and events.json for the overlay.
  async finish() {
    await this.untilQuiet()
    writeFileSync(join(this.voiceDir, "timeline.json"), JSON.stringify(this.timeline, null, 1))
    const viewport = this.page.viewportSize()!
    const events = this.events
      .map((e, order) => ({ e, order }))
      .sort((a, b) => a.e.ts - b.e.ts || a.order - b.order)
      .map(({ e: { ts, ...rest } }) => ({ t: Math.max(0, Math.round(ts - this.t0)) / 1000, ...rest }))
    writeFileSync(join(this.demoDir, "events.json"), JSON.stringify({
      version: 1,
      viewport: { width: viewport.width, height: viewport.height }, // px the events are in
      events,
    }, null, 1))
  }

  // ---- overlay ------------------------------------------------------------------------------

  async ready() {
    await this.page.waitForLoadState("domcontentloaded")
    await this.page.evaluate(() => document.fonts.ready)
  }

  async caption(html: string) {
    this.emit({ type: "caption", ...(html ? { id: this.id("caption") } : {}), html })
  }

  // Caption for the next step, numbered when sequence.json has step_numbers.
  async stepCaption(html: string) {
    this.step += 1
    this.emit({
      type: "caption", id: `step-${this.step}`,
      html: this.seq.step_numbers ? `<span class="step">${this.step}</span>${html}` : html,
    })
  }

  // Highlight box around one field or button (never a whole form). Options:
  //   spotlight: dim the rest of the page, for pages with a lot of text
  //   zoom:      scale the page around the element (true = theme's scale), for busy pages
  // Each call undoes the previous zoom first, so a scene never starts zoomed.
  async highlight(target: Locator | null, opts: HighlightOptions = {}) {
    await this.ready()
    await this.unzoom()
    if (!target) {
      this.emit({ type: "box", rect: null })
      this.boxShown = false
      await this.page.evaluate(() => ((window as any).__demoTracked = undefined))
      return
    }
    // The old box goes before the page moves (scroll, zoom), or it would frame the wrong spot.
    if (this.boxShown) this.emit({ type: "box", rect: null })
    await target.scrollIntoViewIfNeeded()
    const handle = await target.elementHandle()
    if (opts.zoom) {
      const scale = opts.zoom === true ? this.layout.focus_zoom.scale : opts.zoom
      this.emit({ type: "cursor-visible", on: false }) // it would point at the unzoomed page
      // Scale the page and move the element to the middle of the screen (a bit above, clear of the
      // caption), so a zoom never pushes it off an edge. Never so far that the element overflows.
      await this.page.evaluate(({ el, scale }) => {
        const r = (el as Element).getBoundingClientRect()
        const b = document.body.getBoundingClientRect()
        const s = Math.max(1, Math.min(scale, (innerWidth * 0.9) / r.width, (innerHeight * 0.6) / r.height))
        const tx = innerWidth / 2 - b.left - s * (r.left + r.width / 2 - b.left)
        const ty = innerHeight * 0.42 - b.top - s * (r.top + r.height / 2 - b.top)
        document.body.style.transformOrigin = "0 0"
        document.body.style.transform = `translate(${tx}px, ${ty}px) scale(${s})`
      }, { el: handle, scale })
      await this.page.waitForTimeout(this.layout.focus_zoom.ms + 50)
      this.zoomed = true
    }
    const rect: Rect = await this.page.evaluate((el) => {
      ;(window as any).__demoTracked = el
      const r = (el as Element).getBoundingClientRect()
      return { x: r.left, y: r.top, w: r.width, h: r.height }
    }, handle)
    this.emit({ type: "box", rect, spot: !!opts.spotlight, ...(opts.pad !== undefined ? { pad: opts.pad } : {}) })
    this.boxShown = true
    if (opts.spotlight) await this.page.waitForTimeout(this.layout.spotlight.fade_ms)
  }

  private async unzoom() {
    if (!this.zoomed) return
    this.emit({ type: "box", rect: null })
    this.boxShown = false
    this.emit({ type: "cursor-visible", on: true })
    await this.page.evaluate(() => {
      ;(window as any).__demoTracked = undefined
      document.body.style.transform = ""
    })
    await this.page.waitForTimeout(this.layout.focus_zoom.ms + 50)
    this.zoomed = false
  }

  private async clearStage() {
    await this.caption("")
    await this.highlight(null)
  }

  // Navigate behind a veil: fade the page to the card colour, run `go` (a click that loads a page,
  // or a goto), and fade the new page in. Use it for every page change after the intro, or a dark
  // page cuts straight to a light one (a FLASH). Caption and highlight go with the old page.
  async navigate(go: () => Promise<unknown>) {
    await this.ready()
    await this.unzoom()
    this.emit({ type: "veil", on: true })
    await this.page.waitForTimeout(VEIL)
    this.emit({ type: "caption", html: "" })
    this.emit({ type: "box", rect: null })
    this.boxShown = false
    await go()
    await this.page.waitForLoadState("domcontentloaded")
    await this.ready()
    this.emit({ type: "veil", on: false })
    await this.page.waitForTimeout(VEIL + 100)
  }

  goto(url: string) {
    return this.navigate(() => this.page.goto(url))
  }

  // Scroll so the target sits in the upper-middle of the screen, clear of the caption.
  async scrollTo(target: Locator) {
    await target.evaluate((el) => {
      const r = el.getBoundingClientRect()
      window.scrollTo({ top: window.scrollY + r.top - window.innerHeight * 0.35, behavior: "smooth" })
    })
    await this.page.waitForTimeout(1200)
  }

  async moveTo(target: Locator) {
    const b = (await target.boundingBox())!
    await this.page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 30 })
    await this.page.waitForTimeout(500)
  }

  async click(target: Locator) {
    await this.moveTo(target)
    this.emit({ type: "press", down: true })
    await target.click()
    this.emit({ type: "press", down: false })
  }

  // Full-screen card. Use .tag, h1, p, ul > li (li.warn for a caution). Fades in unless instant.
  // kind (intro, chapter, summary, outro) picks the card style in theme.cards.
  async showCard(html: string, { instant = false, kind }: { instant?: boolean; kind?: (typeof CARD_KINDS)[number] } = {}) {
    this.emit({ type: "card", id: this.id("card"), html, ...(kind ? { kind } : {}), instant })
    this.cardShown = true
    if (!instant) await this.page.waitForTimeout(this.layout.fade_ms)
  }

  async hideCard() {
    if (!this.cardShown) return
    this.emit({ type: "card-hide" })
    await this.page.waitForTimeout(this.layout.fade_ms)
    this.emit({ type: "card-remove" })
    this.cardShown = false
  }
}
