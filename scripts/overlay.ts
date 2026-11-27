// The overlay every walkie draws: caption bar, highlight box, spotlight, cursor, cards and the
// navigation veil, styled from the profile's theme. It runs inside the browser (no imports, no
// Node), so the render (render.ts) and the configurator preview (preview.ts) share it: what the
// preview shows is what the video shows.
//
// Sizes in theme.json are pixels of the final video. The page is shown at `zoom` (1.5 = 150 %), so
// one CSS pixel is `zoom` video pixels; the overlay divides by zoom to keep sizes as written.

// Card style (theme.cards): `all` applies to every card, and intro / chapter / summary / outro
// override it field by field. Image paths are absolute (wt profile resolves them).
export type CardStyle = {
  align?: "center" | "left"; padding_x?: number; tag?: "bottom" | "top" | "none"
  logo?: string | null; logo_position?: "top-left" | "top-right" | "bottom-left" | "bottom-right" | "above-title"
  logo_height?: number; background?: string | null; background_image?: string | null
}
export const VEIL = 600 // ms for each half of a navigation fade: white to near-black stays under the flash limit
export const CARD_KINDS = ["intro", "chapter", "summary", "outro"] as const

export type Profile = {
  theme: {
    cards?: { all?: CardStyle } & Partial<Record<(typeof CARD_KINDS)[number], CardStyle>>
    colors: Record<string, string>
    fonts: { sans: { medium: string; bold: string }; mono: { regular: string; bold: string } }
    layout: any
    hide?: string[] // selectors of dev-only UI to hide in the video (toolbars, profilers)
  }
  sequence: {
    intro: boolean; chapter: boolean; summary: boolean; outro: boolean
    step_numbers: boolean; chapter_ms: number; outro_html: string
  }
}

// Draws the overlay on a page: in the render (render.ts, over nothing, as transparent frames laid
// over the recorded page) and in the configurator preview. Never on the page being recorded: that
// gets contentScript, and the recording keeps the overlay as events (events.json), so the page,
// the overlay and the text stay separate layers. Overlay elements hang off <html>, not <body>.
// assets: image path -> CSS url() value (data URL), for card logos and backgrounds.
// stage: a device recording's layout (wt mix's stage()): the device screen's place, and in the
// side layout the text panel's, in video px. The caption moves into the panel; the veil and the
// spotlight stay on the screen.
type Box = { x: number; y: number; w: number; h: number }
export type Stage = {
  layout: "side" | "full"; phone: Box; panel: Box | null
  radius: number; bezel: number; caption_size: number
}
export const overlayScript = ({ t, fonts, assets = {}, VEIL, stage = null }: {
  t: Profile["theme"]; fonts: Record<string, string>; assets?: Record<string, string>; VEIL: number; stage?: Stage | null
}) => {
  const install = () => {
    if (document.getElementById("demo-style")) return
    const L = t.layout
    const c = t.colors
    const px = (v: number) => `${v / L.zoom}px`
    // Shape: square is the technical-document look (square corners, mono capitals on the tag band
    // under a rule, in the profile's sans font); round keeps the rounded bar and plate and a round step badge.
    const round = (L.shape ?? "square") === "round"
    const r = (v: number) => (round ? px(v) : "0")
    // Device recordings: the stage paints everything but the screen (a rounded hole with a frame
    // line); the veil and the spotlight's dimming cover only the screen; in the side layout the
    // caption is the panel's text, in the full layout it is the web's caption bar.
    const stageCss = (s: Stage) => {
      const p = s.phone
      const at = `left: ${p.x}px; top: ${p.y}px; width: ${p.w}px; height: ${p.h}px; border-radius: ${s.radius}px;`
      const panel = s.panel
      return `
      #demo-stage { position: fixed; ${at} z-index: 2147483643; pointer-events: none;
        box-shadow: 0 0 0 ${s.bezel}px ${c.primary}, 0 0 0 200vmax ${c.card_wash}; }
      #demo-veil { inset: auto; ${at} }
      #demo-dim { position: fixed; ${at} z-index: 2147483644; overflow: hidden; pointer-events: none; }
      #demo-hole { position: absolute; border-radius: ${px(L.highlight.radius)}; box-shadow: 0 0 0 200vmax rgba(0,0,0,0);
        transition: left .45s ease, top .45s ease, width .45s ease, height .45s ease,
          box-shadow ${L.spotlight.fade_ms}ms ease-in-out; }
      #demo-hole.spot { box-shadow: 0 0 0 200vmax rgba(0,0,0,${L.spotlight.dim}); }
      #demo-hole.follow { transition: box-shadow ${L.spotlight.fade_ms}ms ease-in-out; }
      #demo-box.spot { box-shadow: none; }
      ${panel ? panelCss(s, panel) : ""}`
    }
    const panelCss = (s: Stage, panel: Box) => `
      #demo-caption { left: ${panel.x}px; top: 50%; bottom: auto; right: auto; transform: translateY(-50%);
        width: ${panel.w}px; max-width: none; box-sizing: border-box; padding: 0 ${px(24)};
        background: none; color: ${c.card_text}; text-align: left; border-radius: 0;
        font-size: ${s.caption_size}px; line-height: 1.5; }
      #demo-caption b { color: ${c.primary};
        background: linear-gradient(transparent 62%, color-mix(in srgb, ${c.emphasis} 70%, transparent) 62%); }
      #demo-caption .step { display: block; width: 1.6em; min-width: 0; height: 1.6em; line-height: 1.6em;
        margin: 0 0 .5em; text-align: center; background: ${c.primary}; color: ${c.on_primary}; font-size: .7em; }`
    // One rule set per card kind: `all` styles the bare #demo-card, each kind overrides it.
    const cards: any = t.cards ?? {}
    const cardCss = (["all", "intro", "chapter", "summary", "outro"] as const).map((kind) => {
      const own = kind === "all" ? {} : (cards[kind] ?? {})
      const st = { align: "left", padding_x: 140, tag: "top", logo: null, logo_position: "top-left",
        logo_height: 56, background: null, background_image: null, ...(cards.all ?? {}), ...own }
      const sel = kind === "all" ? "#demo-card" : `#demo-card.kind-${kind}`
      const left = st.align === "left"
      const image = st.background_image && assets[st.background_image] ? ` ${assets[st.background_image]} center / cover no-repeat` : ""
      const corner = String(st.logo_position).split("-") // a corner on the tag band's edge moves clear of it
      return `
      ${sel} { background: ${st.background ?? c.card_background}${image}; text-align: ${st.align};
        align-items: ${left ? "flex-start" : "center"};
        padding: ${st.tag === "top" ? px(96) : 0} ${px(st.padding_x)} ${st.tag === "bottom" ? px(96) : 0}; }
      ${sel} ul { margin-inline: ${left ? 0 : "auto"}; }
      ${sel} .tag { ${st.tag === "none" ? "display: none;" : st.tag === "top" ? "top: 0; bottom: auto;" : "top: auto; bottom: 0;"}
        justify-content: ${left ? "flex-start" : "center"}; padding-inline: ${left ? px(st.padding_x) : 0}; }
      ${sel} .tag::after { ${st.tag === "top" ? "top: auto; bottom: 0;" : "top: 0; bottom: auto;"} }
      ${sel} .logo { display: ${st.logo && assets[st.logo] ? "block" : "none"}; height: ${px(st.logo_height)}; width: auto;
        ${st.logo_position === "above-title" ? "position: static;" : `position: absolute; ${corner[0]}: ${px(corner[0] === st.tag ? 136 : 40)}; ${corner[1]}: ${px(48)};`} }
      ${sel} .logo { content: ${st.logo && assets[st.logo] ? assets[st.logo] : "none"}; }`
    }).join("")
    const style = document.createElement("style")
    style.id = "demo-style"
    style.textContent = `
      @font-face { font-family: DemoSans; font-weight: 500; src: ${fonts.medium}; }
      @font-face { font-family: DemoSans; font-weight: 700; src: ${fonts.bold}; }
      @font-face { font-family: DemoMono; font-weight: 400; src: ${fonts.monoRegular}; }
      @font-face { font-family: DemoMono; font-weight: 700; src: ${fonts.monoBold}; }
      .demo-ui { font-family: DemoSans, DemoMono, sans-serif; }
      .demo-ui code, .demo-ui kbd, .demo-ui .mono { font-family: DemoMono, monospace; }
      #demo-caption { position: fixed; left: 50%; ${L.caption.position}: ${px(L.caption.margin)};
        transform: translateX(-50%); width: max-content; max-width: ${px(1180)};
        padding: ${px(14)} ${px(28)}; border-radius: ${r(14)}; z-index: 2147483646;
        background: ${c.primary}; color: ${c.on_primary};
        font-weight: 500; font-size: ${px(L.caption.size)}; line-height: 1.45; text-align: center;
        transition: opacity .3s; pointer-events: none; }
      #demo-caption:empty { opacity: 0; }
      #demo-caption b { color: ${c.emphasis}; font-weight: 700; white-space: nowrap; }
      #demo-caption .step { display: inline-block; min-width: 1.5em; height: 1.5em; line-height: 1.5em;
        margin-right: .5em; border-radius: ${round ? "50%" : "0"}; background: ${c.emphasis}; color: ${c.primary};
        font-weight: 700; font-size: .8em; vertical-align: .1em; }
      #demo-box { position: fixed; z-index: 2147483645; pointer-events: none;
        border: ${px(L.highlight.width)} solid ${c.highlight}; border-radius: ${px(L.highlight.radius)};
        opacity: 0; box-shadow: 0 0 0 200vmax rgba(0,0,0,0);
        transition: opacity .3s, left .45s ease, top .45s ease, width .45s ease, height .45s ease,
          box-shadow ${L.spotlight.fade_ms}ms ease-in-out; }
      #demo-box.spot { box-shadow: 0 0 0 200vmax rgba(0,0,0,${L.spotlight.dim}); }
      #demo-box.follow { transition: opacity .3s, box-shadow ${L.spotlight.fade_ms}ms ease-in-out; }
      #demo-cursor { position: fixed; z-index: 2147483647; width: ${px(22)}; height: ${px(22)};
        margin: ${px(-11)} 0 0 ${px(-11)}; border-radius: 50%; pointer-events: none;
        background: ${c.highlight}99; border: 2px solid #fff; transition: transform .12s;
        left: -50px; top: -50px; }
      #demo-cursor.down { transform: scale(.6); }
      /* Card "plate" (chosen with hallmark): centred head, meta on a wash band at the foot,
         summary list on a wash plate; check and warn marks drawn in CSS, never emoji. */
      #demo-card { position: fixed; inset: 0; z-index: 2147483647; display: flex; box-sizing: border-box;
        flex-direction: column; align-items: center; justify-content: center; gap: ${px(28)};
        padding: 0 ${px(140)} ${px(96)}; text-align: center;
        background: ${c.card_background}; color: ${c.card_text};
        opacity: 0; transition: opacity ${L.fade_ms}ms ease-in-out; }
      #demo-card.shown { opacity: 1; }
      ${cardCss}
      #demo-card .tag { position: absolute; left: 0; right: 0; bottom: 0; display: flex;
        justify-content: center; gap: ${px(48)}; padding: ${px(28)} 0; background: ${c.card_wash};
        font-weight: 500; font-size: ${px(L.card.tag)};
        color: color-mix(in srgb, ${c.card_text} 75%, ${c.card_background}); }
      #demo-card h1 { font-weight: 700; font-size: ${px(L.card.title)}; line-height: 1.25; margin: 0;
        max-width: ${px(980)}; text-wrap: balance; overflow-wrap: break-word; min-width: 0; }
      #demo-card p { font-weight: 500; font-size: ${px(L.card.text)}; line-height: 1.5; margin: 0;
        max-width: ${px(900)}; color: color-mix(in srgb, ${c.card_text} 75%, ${c.card_background}); }
      #demo-card ul { list-style: none; margin: 0; display: grid; gap: ${px(20)}; text-align: left;
        background: ${c.card_wash}; border-radius: ${r(20)}; padding: ${px(36)} ${px(56)};
        font-weight: 500; font-size: ${px(L.card.list)}; line-height: 1.45; }
      #demo-card li { position: relative; padding-left: ${px(56)}; }
      #demo-card li::before { content: ""; position: absolute; left: ${px(6)}; top: .32em;
        width: ${px(14)}; height: ${px(24)}; border: solid currentColor;
        border-width: 0 ${px(5)} ${px(5)} 0; transform: rotate(45deg); }
      #demo-card li.warn { color: ${c.warn}; }
      #demo-card li.warn::before { left: 0; top: .2em; width: ${px(30)}; height: ${px(30)}; border: 0;
        border-radius: ${round ? "50%" : "0"}; background: ${c.warn}; transform: none; }
      #demo-card li.warn::after { content: "!"; position: absolute; left: 0; top: .2em; width: ${px(30)};
        line-height: ${px(30)}; text-align: center; color: ${c.card_background}; font-weight: 700;
        font-size: ${px(22)}; }
      /* Veil: fades the page to the card colour and back across a navigation, so a dark page never
         cuts straight to a light one. Under the caption, over the page. */
      ${round ? "" : `
      #demo-card .tag { font-family: DemoSans, sans-serif; font-weight: 500; text-transform: uppercase;
        letter-spacing: .1em; font-size: ${px(L.card.tag * 0.85)}; background: none;
        color: ${c.card_text}; padding-block: ${px(32)}; }
      #demo-card .tag::after { content: ""; position: absolute; left: ${px(48)}; right: ${px(48)};
        height: ${px(3)}; background: ${c.card_text}; }
      #demo-card .tag::before { content: ""; position: absolute; left: 0; top: 0; bottom: 0;
        width: ${px(12)}; background: ${c.emphasis}; }
      #demo-card h1 { letter-spacing: -0.02em; }
      #demo-card ul { border-left: ${px(8)} solid ${c.emphasis}; }`}
      #demo-veil { position: fixed; inset: 0; z-index: 2147483644; pointer-events: none;
        background: ${c.card_background}; opacity: 0; transition: opacity ${VEIL}ms linear; }
      #demo-veil.on { opacity: 1; }
      /* Thai has no spaces between words, and Chrome's dictionary splits loanwords mid-word
         (ริ / สต์แบนด์). Overlay text breaks only at the spaces the author wrote. */
      .demo-phrase { white-space: nowrap; }
      ${stage ? stageCss(stage) : ""}
    `
    document.head.appendChild(style)
    for (const id of [...(stage ? ["demo-stage", "demo-dim"] : []), "demo-veil", "demo-caption", "demo-box", "demo-cursor"]) {
      const el = document.createElement("div")
      el.id = id
      el.className = "demo-ui"
      document.documentElement.appendChild(el)
    }
    if (stage) {
      const hole = document.createElement("div")
      hole.id = "demo-hole"
      document.getElementById("demo-dim")!.appendChild(hole)
    }
    // A full-screen card of one kind (intro, chapter, summary, outro), not yet on the page. The
    // logo comes from the profile, so specs never pass one.
    ;(window as any).__demoCard = (html: string, kind?: string) => {
      const card = document.createElement("div")
      card.id = "demo-card"
      card.className = "demo-ui" + (kind ? ` kind-${kind}` : "")
      card.innerHTML = html
      const logo = document.createElement("img")
      logo.className = "logo"
      logo.alt = ""
      card.prepend(logo)
      ;(window as any).__demoPhrases(card)
      return card
    }
    // Wrap each space-separated run of text in a nowrap span (see .demo-phrase).
    ;(window as any).__demoPhrases = (root: Element) => {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
      const nodes: Text[] = []
      while (walker.nextNode()) nodes.push(walker.currentNode as Text)
      for (const node of nodes) {
        if (!node.data.trim() || node.parentElement?.closest(".demo-phrase, .step")) continue
        const frag = document.createDocumentFragment()
        for (const part of node.data.split(/(\s+)/)) {
          if (!part) continue
          if (/^\s+$/.test(part)) frag.append(part)
          else {
            const span = document.createElement("span")
            span.className = "demo-phrase"
            span.textContent = part
            frag.append(span)
          }
        }
        node.replaceWith(frag)
      }
    }
    // Draw one event of a recording (see Demo in demo.ts). Coordinates are px of this page; the
    // caller scales them from the recorded page when this page is a different size.
    ;(window as any).__demoApply = (e: any) => {
      const $ = (id: string) => document.getElementById(id)!
      const pad = px(e.pad ?? L.highlight.pad)
      const place = (box: HTMLElement, r: any) => Object.assign(box.style, {
        left: `calc(${r.x}px - ${pad})`, top: `calc(${r.y}px - ${pad})`,
        width: `calc(${r.w}px + 2 * ${pad})`, height: `calc(${r.h}px + 2 * ${pad})`,
      })
      // Device recordings: the spotlight is a hole in a layer clipped to the screen, so it dims
      // the screen and not the panel. It moves with the box.
      const hole = (r: any, how: { spot?: boolean; follow?: boolean }) => {
        if (!stage) return
        const h = $("demo-hole")
        h.classList.toggle("follow", !!how.follow)
        if (r) place(h, { ...r, x: r.x - stage.phone.x, y: r.y - stage.phone.y })
        if (how.spot !== undefined) h.classList.toggle("spot", how.spot)
      }
      switch (e.type) {
        case "caption": {
          const el = $("demo-caption")
          el.innerHTML = e.html ?? ""
          ;(window as any).__demoPhrases(el)
          break
        }
        case "box": {
          const box = $("demo-box")
          box.classList.remove("follow")
          if (!e.rect) {
            box.classList.remove("spot")
            box.style.opacity = "0"
            hole(null, { spot: false })
            break
          }
          // On a device stage, a box that was hidden appears in place; it does not glide over from
          // where the last one was (the phone is small, and the glide reads as a wrong target).
          const fresh = !!stage && box.style.opacity !== "1"
          if (fresh) box.classList.add("follow")
          place(box, e.rect)
          box.style.opacity = "1"
          box.classList.toggle("spot", !!e.spot)
          hole(e.rect, { spot: !!e.spot, follow: fresh })
          if (fresh) {
            box.getBoundingClientRect() // commit the place before the glide comes back
            box.classList.remove("follow")
            $("demo-hole").classList.remove("follow")
          }
          break
        }
        case "box-move": // the page scrolled under the box: follow at once, no glide
          $("demo-box").classList.add("follow")
          place($("demo-box"), e.rect)
          hole(e.rect, { follow: true })
          break
        case "cursor":
          Object.assign($("demo-cursor").style, { left: `${e.x}px`, top: `${e.y}px` })
          break
        case "press":
          $("demo-cursor").classList.toggle("down", !!e.down)
          break
        case "cursor-visible":
          $("demo-cursor").style.opacity = e.on ? "" : "0"
          break
        case "veil":
          $("demo-veil").classList.toggle("on", !!e.on)
          break
        case "card": {
          document.getElementById("demo-card")?.remove()
          const card = (window as any).__demoCard(e.html, e.kind) as HTMLElement
          if (e.instant) card.style.transition = "none"
          document.documentElement.appendChild(card)
          card.getBoundingClientRect() // commit opacity 0 so the fade runs
          card.classList.add("shown")
          if (e.instant) {
            card.getBoundingClientRect()
            card.style.transition = ""
          }
          break
        }
        case "card-hide":
          document.getElementById("demo-card")?.classList.remove("shown")
          break
        case "card-remove":
          document.getElementById("demo-card")?.remove()
          break
      }
    }
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", install)
  } else {
    install()
  }
}

// Injected into the page being recorded: only what belongs to the page layer (dev UI hidden, no
// scrollbar, the sync marker's style, a smooth focus zoom), plus reports to the recorder through the
// __demoEvent binding: pointer moves, and the highlighted element's place when the page scrolls.
export const contentScript = ({ hide, zoomMs }: { hide: string[]; zoomMs: number }) => {
  const install = () => {
    if (document.getElementById("demo-content-style")) return
    const style = document.createElement("style")
    style.id = "demo-content-style"
    style.textContent = `
      html { scrollbar-width: none; }
      html::-webkit-scrollbar { display: none; }
      body { transition: transform ${zoomMs}ms ease-in-out; }
      ${hide.length ? `${hide.join(", ")} { display: none !important; }` : ""}
      #demo-marker { position: fixed; left: 0; top: 0; width: 16px; height: 16px; z-index: 2147483647;
        background: #ff00ff; }`
    document.head.appendChild(style)
  }
  const report = (e: any) => (window as any).__demoEvent?.({ ...e, ts: Date.now() })
  document.addEventListener("mousemove", (e) => report({ type: "cursor", x: e.clientX, y: e.clientY }), true)
  let queued = false
  const follow = () => {
    const el = (window as any).__demoTracked as Element | undefined
    if (queued || !el?.isConnected) return
    queued = true
    requestAnimationFrame(() => {
      queued = false
      const r = el.getBoundingClientRect()
      report({ type: "box-move", rect: { x: r.left, y: r.top, w: r.width, h: r.height } })
    })
  }
  window.addEventListener("scroll", follow, true)
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", install)
  else install()
}
