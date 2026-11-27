// Configurator preview: one self-contained HTML page showing every state of a walkie (intro
// card, numbered caption with the highlight box, spotlight, focus zoom, chapter, summary, outro) on a
// sample page, and a phone recording's stage (theme.layout.device), drawn by the same overlay code
// the recording uses (overlay.ts).
//
// Usage: node scripts/preview.ts <profile name | resolved profile.json> [out.html]
//        (default out: preview.html in the current folder)
//
// Live updates, without a reload: the page exposes
//   window.walkiePreview.render(profile, { fonts?, assets?, backdrop? })
//     profile  = { theme, sequence } (resolved, as wt profile --resolve prints it)
//     fonts    = { medium, bold, monoRegular, monoBold }, each a CSS src value such as
//                `url(data:font/woff2;base64,...)`; a key left out keeps the font it had (a custom
//                font previews live this way, before it is copied into the profile)
//     assets   = image path -> CSS url() value, for card logos and backgrounds the profile names
//                (merged into the ones built in)
//     backdrop = an image URL (a clean still from a library video's backdrops/) to draw over
//                instead of the sample page; null goes back to the sample page
//     only     = one state id (intro, step, spotlight, zoom, chapter, summary, outro) to show alone,
//                at full width (an editor's large preview); null shows them all again
// and also accepts window.postMessage({ type: "walkie-profile", profile, fonts, assets, backdrop, only }).
// After each render it tells its parent window which states there are and which the sequence turns
// off: postMessage({ type: "walkie-preview-states", states: [{ id, label, on }] }).
import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, writeFileSync } from "node:fs"
import { extname, join, resolve } from "node:path"
import { cardAssets } from "./assets.ts"
import { overlayScript, VEIL } from "./overlay.ts"
import { WT } from "./paths.ts"



export function loadResolved(nameOrFile: string) {
  if (existsSync(nameOrFile)) return JSON.parse(readFileSync(nameOrFile, "utf8"))
  return JSON.parse(execFileSync(WT, ["profile", "--resolve", nameOrFile], { encoding: "utf8" }))
}

const fontData = (path: string) =>
  `url(data:font/${extname(path).slice(1)};base64,${readFileSync(path).toString("base64")})`

// A neutral back-office page to draw the overlay on. Its own look stays plain on purpose; its text
// is in the profile's sans font (DemoSans, which the overlay loads into the same frame).
const SAMPLE = `<!doctype html><html lang="th"><head><meta charset="utf-8"><style>
  * { box-sizing: border-box; }
  body { margin: 0; font: 14px/1.5 DemoSans, -apple-system, "Helvetica Neue", sans-serif; color: #2b2f3a; background: #f5f6f8; }
  header { height: 44px; background: #fff; border-bottom: 1px solid #e3e5ea; display: flex; align-items: center;
    padding: 0 20px; gap: 18px; font-weight: 600; }
  header span { color: #8a8f9c; font-weight: 400; }
  main { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; padding: 18px 20px; }
  section { background: #fff; border: 1px solid #e3e5ea; border-radius: 8px; padding: 14px 16px; }
  h2 { font-size: 15px; margin: 0 0 10px; }
  label { display: block; font-size: 12px; color: #6b7080; margin: 8px 0 3px; }
  input { width: 100%; height: 30px; border: 1px solid #cfd3dc; border-radius: 5px; padding: 0 8px; font: inherit; }
  button { margin-top: 12px; height: 32px; padding: 0 16px; border: 0; border-radius: 5px; background: #3b5bdb;
    color: #fff; font: inherit; font-weight: 600; }
  p { margin: 0 0 8px; color: #555b68; font-size: 13px; }
  .note { margin-top: 8px; padding: 8px 10px; background: #fff8e6; border-radius: 5px; font-size: 12px; }
</style></head><body>
  <header>Back office <span>Events</span><span>Orders</span><span>Settings</span></header>
  <main>
    <section>
      <h2>Notification settings</h2>
      <label>Email</label><input id="email" value="staff@example.com">
      <label>Send at</label><input value="09:00">
      <button id="save">Save</button>
    </section>
    <section id="policy">
      <h2>Refund policy</h2>
      <p>Refunds are allowed up to 7 days before the event. After that, organizers decide case by case.</p>
      <p>Wristbands already scanned at the gate cannot be refunded. Top-ups stay on the wristband until the event ends.</p>
      <p>Each request needs the order number and the payment reference.</p>
      <div class="note" id="note">Changes apply to new orders only.</div>
    </section>
  </main>
</body></html>`

const INTRO = `<div class="tag"><span>TICKET-0000</span><span>ตัวอย่าง</span></div>
  <h1>ตั้งค่าการแจ้งเตือน ทางอีเมล</h1><p>วิดีโอสาธิต สำหรับทีมซัพพอร์ต <code>mono</code></p>`
const CHAPTER = `<h1>หลังแก้</h1>`
const SUMMARY = `<h1>สรุป</h1><ul><li>ตั้งอีเมล ที่รับการแจ้งเตือนได้</li><li>บันทึกแล้ว มีผลทันที</li>
  <li class="warn">คำสั่งซื้อเก่า ไม่ได้รับการแจ้งเตือน</li></ul>`

// Runs in the preview page. Each state is an iframe the size of the recorded viewport, scaled down.
const client = ({ overlay, fonts, assets, sample, cards, VEIL }: {
  overlay: string; fonts: Record<string, string>; assets: Record<string, string>; sample: string
  cards: Record<string, string>; VEIL: number
}) => {
  // Over a real still: the same targets, at fixed spots, so the box and zoom still have something to frame.
  // The still is a div, not the body's background: that one paints the canvas and would not zoom.
  const backdropPage = (url: string) => `<!doctype html><html><head><meta charset="utf-8"><style>
    html, body { margin: 0; height: 100%; overflow: hidden; }
    #still { position: absolute; inset: 0; background: url("${url}") center / cover no-repeat; }
    #save, #policy, #email { position: absolute; }
    #save { left: 30%; top: 46%; width: 12%; height: 8%; }
    #policy { left: 52%; top: 22%; width: 40%; height: 40%; }
    #email { left: 20%; top: 30%; width: 30%; height: 7%; }
  </style></head><body><div id="still"></div><div id="save"></div><div id="policy"></div><div id="email"></div></body></html>`
  type State = { id: string; label: string; when?: (s: any) => boolean; device?: boolean; apply: (w: any, d: Document, p: any) => void }
  // A phone recording's stage: the same layout as wt mix's stage(), for a 440 x 956 pt phone.
  const PHONE = { width: 440, height: 956 }
  const phoneStage = (p: any) => {
    const [W, H] = p.theme.layout.video
    const dev = { side: "left", margin: 40, radius: 36, bezel: 6, caption_size: 40, ...(p.theme.layout.device ?? {}) }
    const m = dev.margin
    const ph = Math.floor((H - 2 * m) / 2) * 2
    const pw = Math.round((ph * PHONE.width) / PHONE.height / 2) * 2
    const col = Math.max(pw + 2 * m, Math.round(W * 0.4))
    const left = dev.side === "left"
    return {
      layout: "side", radius: dev.radius, bezel: dev.bezel, caption_size: dev.caption_size,
      phone: { x: left ? Math.floor((col - pw) / 2) : W - col + Math.floor((col - pw) / 2), y: m, w: pw, h: ph },
      panel: { x: left ? col : m, y: m, w: W - col - m, h: H - 2 * m },
    }
  }
  // A plain app screen in the phone's place, standing in for the recording.
  const phonePage = (st: any) => {
    const { x, y, w, h } = st.phone
    const k = w / PHONE.width
    return `<!doctype html><html><head><meta charset="utf-8"><style>
    html, body { margin: 0; height: 100%; overflow: hidden; background: #000; }
    #screen { position: absolute; left: ${x}px; top: ${y}px; width: ${PHONE.width}px; height: ${PHONE.height}px;
      transform: scale(${k}); transform-origin: 0 0; background: #fdf8fc; font: 16px DemoSans, -apple-system, sans-serif; color: #1d1b20; }
    .bar { height: 54px; } h1 { font: 22px DemoSans, -apple-system, sans-serif; margin: 12px 16px 20px; }
    .field { margin: 0 16px; height: 56px; border-bottom: 1px solid #79747e; color: #49454f; padding-top: 8px; box-sizing: border-box; }
    #save { margin: 12px 16px; height: 40px; border-radius: 20px; background: #6750a4; color: #fff; text-align: center; line-height: 40px; }
    .row { margin: 0 16px; padding: 14px 16px; }
  </style></head><body><div id="screen"><div class="bar"></div><h1>Notifications</h1>
    <div class="field" id="email">Email</div><div id="save">Save</div>
    ${Array.from({ length: 14 }, (_, i) => `<div class="row">Wristband ${i + 1}</div>`).join("")}</div></body></html>`
  }
  const card = (html: string, kind: string) => (w: any, d: Document) => {
    const el = w.__demoCard(html, kind)
    el.classList.add("shown")
    d.documentElement.appendChild(el)
  }
  const caption = (w: any, d: Document, p: any, html: string, n: number) => {
    const el = d.getElementById("demo-caption")!
    el.innerHTML = (p.sequence.step_numbers ? `<span class="step">${n}</span>` : "") + html
    w.__demoPhrases(el)
  }
  const box = (d: Document, p: any, sel: string, spot = false) => {
    const r = d.querySelector(sel)!.getBoundingClientRect()
    const pad = p.theme.layout.highlight.pad / p.theme.layout.zoom
    Object.assign(d.getElementById("demo-box")!.style, {
      left: `${r.left - pad}px`, top: `${r.top - pad}px`, width: `${r.width + pad * 2}px`, height: `${r.height + pad * 2}px`, opacity: "1",
    })
    d.getElementById("demo-box")!.classList.toggle("spot", spot)
  }
  const states: State[] = [
    { id: "intro", label: "Intro card", when: (s) => s.intro, apply: card(cards.intro, "intro") },
    { id: "step", label: "Step caption + highlight", apply: (w, d, p) => { caption(w, d, p, "กรอกอีเมล แล้วกด <b>“Save”</b>", 1); box(d, p, "#save") } },
    { id: "spotlight", label: "Spotlight", apply: (w, d, p) => { caption(w, d, p, "อ่าน <b>เงื่อนไขการคืนเงิน</b> ก่อนกด", 2); box(d, p, "#policy", true) } },
    {
      id: "zoom", label: "Focus zoom", apply: (w, d, p) => {
        const el = d.querySelector("#email")!
        const r = el.getBoundingClientRect()
        const s = Math.max(1, Math.min(p.theme.layout.focus_zoom.scale, (w.innerWidth * 0.9) / r.width, (w.innerHeight * 0.6) / r.height))
        const tx = w.innerWidth / 2 - s * (r.left + r.width / 2)
        const ty = w.innerHeight * 0.42 - s * (r.top + r.height / 2)
        d.body.style.transformOrigin = "0 0"
        d.body.style.transform = `translate(${tx}px, ${ty}px) scale(${s})`
        caption(w, d, p, "ใส่ <b>อีเมลของทีม</b>", 3)
        box(d, p, "#email")
      },
    },
    { id: "device", label: "Phone (Flutter) step", device: true, apply: (w, d, p) => { caption(w, d, p, "กรอกอีเมล แล้วกด <b>“Save”</b>", 1); box(d, { ...p, theme: { ...p.theme, layout: { ...p.theme.layout, zoom: 1 } } }, "#save") } },
    { id: "chapter", label: "Chapter card", when: (s) => s.chapter, apply: card(cards.chapter, "chapter") },
    { id: "summary", label: "Summary card", when: (s) => s.summary, apply: card(cards.summary, "summary") },
    { id: "outro", label: "Outro card", when: (s) => s.outro && s.outro_html, apply: (w, d, p) => card(p.sequence.outro_html, "outro")(w, d) },
  ]

  const grid = document.getElementById("grid")!
  let generation = 0
  let current = fonts
  let images = assets
  let backdrop: string | null = null
  let only: string | null = null
  const render = (p: any, opts: { fonts?: Record<string, string>; assets?: Record<string, string>; backdrop?: string | null; only?: string | null } = {}) => {
    if (opts.fonts) current = { ...current, ...opts.fonts }
    if (opts.assets) images = { ...images, ...opts.assets }
    if (opts.backdrop !== undefined) backdrop = opts.backdrop
    if (opts.only !== undefined) only = opts.only
    document.body.classList.toggle("single", !!only)
    const gen = ++generation
    const [vw, vh] = p.theme.layout.video
    const zoom = p.theme.layout.zoom
    const view = { w: Math.round(vw / zoom), h: Math.round(vh / zoom) }
    grid.replaceChildren()
    const info = states.map((st) => ({ id: st.id, label: st.label, on: !st.when || !!st.when(p.sequence) }))
    if (window.parent !== window) window.parent.postMessage({ type: "walkie-preview-states", states: info }, "*")
    for (const st of states) {
      if (only && st.id !== only) continue
      const fig = document.createElement("figure")
      const frame = document.createElement("div")
      frame.className = "frame"
      frame.style.aspectRatio = `${vw} / ${vh}`
      const iframe = document.createElement("iframe")
      // A phone stage is drawn at video size (zoom 1), as render.ts draws it.
      const size = st.device ? { w: vw, h: vh } : view
      const stage = st.device ? phoneStage(p) : null
      const theme = st.device ? { ...p.theme, layout: { ...p.theme.layout, zoom: 1 } } : p.theme
      Object.assign(iframe.style, { width: `${size.w}px`, height: `${size.h}px` })
      const cap = document.createElement("figcaption")
      const on = !st.when || st.when(p.sequence)
      cap.textContent = st.label + (on ? "" : " (off in sequence)")
      fig.className = on ? "" : "off"
      frame.append(iframe)
      fig.append(frame, cap)
      grid.append(fig)
      const fit = () => (iframe.style.transform = `scale(${frame.clientWidth / size.w})`)
      new ResizeObserver(fit).observe(frame)
      iframe.addEventListener("load", () => {
        if (gen !== generation) return
        const w = iframe.contentWindow as any
        const d = iframe.contentDocument!
        new w.Function("a", `(${overlay})(a)`)({ t: theme, fonts: current, assets: images, VEIL, stage })
        const still = d.createElement("style") // a still frame: no fades
        still.textContent = "*, *::before, *::after { transition: none !important; }"
        d.head.appendChild(still)
        st.apply(w, d, p)
      }, { once: true })
      iframe.srcdoc = st.device ? phonePage(stage) : backdrop ? backdropPage(backdrop) : sample
    }
  }
  ;(window as any).walkiePreview = { render, states: states.map((s) => s.id) }
  window.addEventListener("message", (e) => {
    if (e.data?.type === "walkie-profile") render(e.data.profile, e.data)
  })
  render((window as any).__initialProfile)
}

export function previewHtml(profile: any): string {
  const f = profile.theme.fonts
  const fonts = {
    medium: fontData(f.sans.medium), bold: fontData(f.sans.bold),
    monoRegular: fontData(f.mono.regular), monoBold: fontData(f.mono.bold),
  }
  const args = { overlay: overlayScript.toString(), fonts, assets: cardAssets(profile.theme), sample: SAMPLE, cards: { intro: INTRO, chapter: CHAPTER, summary: SUMMARY }, VEIL }
  const json = (v: unknown) => JSON.stringify(v).replace(/</g, "\\u003c")
  return `<!doctype html>
<html lang="th"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Walkie preview</title>
<style>
  :root { color-scheme: light dark; --bg: #f3f4f6; --text: #3a3f4b; --frame: #d5d8de; }
  @media (prefers-color-scheme: dark) { :root { --bg: #1c1d21; --text: #c9ccd4; --frame: #3a3c43; } }
  body { margin: 0; padding: 16px; background: var(--bg); color: var(--text); font: 13px -apple-system, sans-serif; }
  #grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 16px; }
  figure { margin: 0; }
  figure.off { opacity: .45; }
  .frame { position: relative; overflow: hidden; border-radius: 6px; outline: 1px solid var(--frame); }
  .frame iframe { position: absolute; left: 0; top: 0; border: 0; transform-origin: 0 0; }
  figcaption { margin-top: 6px; }
  body.single { padding: 0; background: transparent; }
  body.single #grid { grid-template-columns: 1fr; }
  body.single figcaption { display: none; }
  body.single figure.off { opacity: 1; }
  body.single .frame { border-radius: 0; outline: 0; }
</style></head>
<body><div id="grid"></div>
<script>window.__initialProfile = ${json({ theme: profile.theme, sequence: profile.sequence })};
(${client.toString()})(${json(args)})</script>
</body></html>`
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [which, out = "preview.html"] = process.argv.slice(2)
  if (!which) {
    console.error("usage: node scripts/preview.ts <profile name | resolved profile.json> [out.html]")
    process.exit(2)
  }
  writeFileSync(resolve(out), previewHtml(loadResolved(which)))
  console.log(resolve(out))
}
