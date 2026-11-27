// Walkie MCP server (stdio). Wraps the engine scripts so an agent can storyboard, voice,
// record and manage walkies (narrated videos of a change). Run: node mcp/server.ts (Node 23.6+ strips types).
//
// The flow it serves, with the two human checkpoints:
//   start -> storyboard (show the table, wait for approval) -> write the spec -> record
//   (show the contact sheet, wait for approval) -> hand over the MP4.
// Finished videos are copied to a library on this machine (WALKIE_HOME), one folder each.
import { McpServer } from "@modelcontextprotocol/server"
import { serveStdio } from "@modelcontextprotocol/server/stdio"
import { spawn } from "node:child_process"
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { basename, dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { z } from "zod"
import { LIBRARY, PREVIEWS, WT, checkWt } from "../scripts/paths.ts"
import { inQueue } from "../scripts/queue.ts"
import { agentUsage, describeUsage } from "../scripts/usage.ts"
import { listLibrary, prText, textItems } from "../scripts/library.ts"
import { compareVersions } from "../scripts/version.ts"

const SKILL = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const SCRIPTS = join(SKILL, "scripts")
const NAME = /^[a-z0-9][a-z0-9-]{0,62}$/
const VERSION: string = JSON.parse(readFileSync(join(SKILL, "package.json"), "utf8")).version


type Run = { code: number; out: string; err: string }

function run(cmd: string, args: string[], opts: { cwd?: string; input?: string; env?: Record<string, string>; timeoutMs?: number } = {}): Promise<Run> {
  return new Promise((done) => {
    const child = spawn(cmd, args, { cwd: opts.cwd, env: { ...process.env, ...opts.env } })
    let out = ""
    let err = ""
    const timer = setTimeout(() => child.kill("SIGTERM"), opts.timeoutMs ?? 10 * 60_000)
    child.stdout.on("data", (d) => (out += d))
    child.stderr.on("data", (d) => (err += d))
    child.on("error", (e) => {
      clearTimeout(timer)
      done({ code: -1, out, err: err + String(e) })
    })
    child.on("close", (code) => {
      clearTimeout(timer)
      done({ code: code ?? -1, out, err })
    })
    if (opts.input !== undefined) child.stdin.end(opts.input)
    else child.stdin.end()
  })
}

const text = (s: string) => ({ content: [{ type: "text" as const, text: s }] })
const fail = (s: string) => ({ content: [{ type: "text" as const, text: s }], isError: true })
const tail = (s: string, n = 40) => s.trim().split("\n").slice(-n).join("\n")
const readJson = (p: string) => JSON.parse(readFileSync(p, "utf8"))

function checkRepo(repo: string) {
  const root = resolve(repo)
  if (!existsSync(join(root, ".git"))) throw new Error(`${root} is not a git repository root`)
  return root
}

// Resolve the profile for a repo into dir/profile.json. Returns the resolved profile.
async function resolveProfile(repo: string, dir: string, profile?: string) {
  checkWt()
  const r = await run(WT, ["profile", dir], { cwd: repo, env: profile ? { DEMO_PROFILE: profile } : {} })
  if (r.code !== 0) throw new Error(tail(r.err || r.out))
  return readJson(join(dir, "profile.json"))
}

// The walkie's profile.json is resolved once, at start. Its voice is read again before each
// storyboard run, so lexicon entries added since (walkie:lexicon) are used right away.
async function refreshVoice(dir: string) {
  const prof = readJson(join(dir, "profile.json"))
  const now = await run(WT, ["profile", "--resolve", prof.profile.name])
  if (now.code !== 0) return
  prof.voice = JSON.parse(now.out).voice
  writeFileSync(join(dir, "profile.json"), JSON.stringify(prof, null, 1))
}

// For start: the language the narration must be written in, and the voice that says it.
function voiceLine(v: { language?: string; engine?: string; model?: string; speaker?: string }) {
  return v.language === "en"
    ? `English (Kokoro, speaker ${v.speaker ?? "af_heart"}). Write the narration in English.`
    : `Thai (MakHam, ${v.model ?? "makham_female"}). Write the narration in Thai.`
}

function demoDir(repo: string, name: string) {
  if (!NAME.test(name)) throw new Error(`name must be lower-case letters, digits and dashes: ${name}`)
  for (const workDir of ["tmp/demo", ...candidateWorkDirs()]) {
    const dir = join(repo, workDir, name)
    if (existsSync(join(dir, "profile.json"))) return dir
  }
  throw new Error(`no walkie named ${name} in ${repo}: call start first`)
}

// Every profile on the search path (wt profile decides), name -> folder.
async function profileDirs(): Promise<Record<string, string>> {
  checkWt()
  const r = await run(WT, ["profile", "--list"])
  if (r.code !== 0) throw new Error(tail(r.err))
  return JSON.parse(r.out)
}

let knownWorkDirs: string[] = []

function candidateWorkDirs() {
  return knownWorkDirs
}

async function refreshWorkDirs() {
  knownWorkDirs = Object.values(await profileDirs())
    .map((d) => join(d, "profile.json"))
    .filter(existsSync)
    .map((p) => readJson(p).work_dir as string | undefined)
    .filter((w): w is string => !!w)
}
await refreshWorkDirs()

// The engine's own Playwright: specs run on it whatever the repo has (or lacks).
const PLAYWRIGHT = join(SKILL, "node_modules", ".bin", "playwright")

// Before recording: ffmpeg on PATH, and the Chromium build this Playwright expects (a no-op when
// it is there already, a download the first time).
async function preflight() {
  if ((await run("ffmpeg", ["-version"])).code !== 0) throw new Error("ffmpeg missing: brew install ffmpeg")
  if (!existsSync(PLAYWRIGHT)) throw new Error(`engine packages missing: run ${join(SKILL, "bin", "walkie")} setup`)
  const r = await run(PLAYWRIGHT, ["install", "chromium"], { timeoutMs: 10 * 60_000 })
  if (r.code !== 0) throw new Error(`could not install Chromium:\n${tail(r.err || r.out)}`)
}

type ClipLine = { clip: string; seconds: number; rate: number; phonemes: string; flags: string }

function parseVoice(out: string): ClipLine[] {
  return out
    .split("\n")
    .filter((l) => /^\S+\t[\d.]+s\t/.test(l))
    .map((l) => {
      const [clip, s, rate, phonemes, flags] = l.split("\t")
      return { clip, seconds: parseFloat(s), rate: parseFloat(rate), phonemes, flags: (flags ?? "").trim() }
    })
}

// Every CPU-heavy step (TTS, recording, building, checking, remixing) waits its turn in the machine-wide
// queue, so none runs next to a recording, which is real time and stutters when the CPU is busy.
let waited = 0
async function heavy(label: string, cmd: string, args: string[], opts: Parameters<typeof run>[2] = {}) {
  const q = await inQueue(label, () => run(cmd, args, opts))
  waited += q.waitedMs
  return q.result
}
function waitedNote() {
  const s = Math.round(waited / 1000)
  waited = 0
  return s > 3 ? `Waited ${s}s in the queue behind other walkie work.` : ""
}

async function voice(dir: string, tsv: string) {
  checkWt()
  const r = await heavy(`tts ${basename(dir)}`, WT, ["tts", dir], { input: tsv })
  if (r.code !== 0) throw new Error(tail(r.err))
  return parseVoice(r.out)
}

async function git(repo: string, ...args: string[]) {
  const r = await run("git", ["-C", repo, ...args])
  return r.code === 0 ? r.out.trim() : ""
}

function videoId(repo: string, name: string) {
  return `${basename(repo)}--${name}`
}


// ---------------------------------------------------------------------------------------------

// One server instance per connection: serveStdio calls the factory for each, and negotiates the
// protocol revision (2025-era initialize or 2026-07-28), so every MCP client can connect.
function createServer() {
  const server = new McpServer({ name: "walkie", version: VERSION }, {
    instructions:
      "Makes walkies: narrated videos of a change, step by step, with a voice-over in the profile's language " +
      "(Thai or English; start says which, write the narration in it). " +
      "Flow: start -> storyboard (show the table to the user and wait for approval) -> write the spec " +
      "(demo.spec.ts, or demo_test.dart for a Flutter app) following the profile notes -> start the app (web) -> record (show the contact sheet, wait for approval) -> " +
      "give the MP4 path. Read the walkie skill (SKILL.md) for the kit API and pronunciation rules. " +
      "Cost: every call re-reads the session's context, so run a walkie in a small fresh context: in Claude Code, " +
      "hand it to the `walkie` subagent (Sonnet) with a short brief and relay its two checkpoints to the user; " +
      "elsewhere, start a new session with the brief. After checkpoint 2, change only the layer asked for: narration " +
      "-> revoice, caption or card text -> texts + rerender, look -> profile + rerender; record again only for a " +
      "change of steps or timing.",
  })

  server.registerTool("start", {
    title: "Start a walkie",
    description:
      "Prepare a walkie in a repo: pick the profile (by git remote, or the one named), write kit.ts " +
      "(the spec imports test, expect, Demo and the profile's helpers from ./kit, never from @playwright/test), " +
      "and copy the profile's spec template to demo.spec.ts if none exists. For a Flutter app (record.runner " +
      "flutter, or auto with a Flutter pubspec.yaml) the spec is demo_test.dart instead: an integration test " +
      "with the Dart kit, recorded on an iOS Simulator or Android emulator. Returns the profile's NOTES.md " +
      "(how this repo starts the app, seeds data and logs in), which the spec must follow.",
    annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    inputSchema: z.object({
      repo: z.string().describe("Absolute path to the repo root"),
      name: z.string().describe("Short name for this walkie, e.g. ticket-123-checkout (lower case, dashes)"),
      profile: z.string().optional().describe("Profile name; omit to pick by git remote"),
      min_engine: z.string().optional().describe("The oldest engine version the calling skill works with, e.g. 0.1.1-alpha"),
    }),
  }, async ({ repo, name, profile, min_engine }) => {
    try {
      if (min_engine && compareVersions(VERSION, min_engine) < 0)
        return fail(`The walkie engine is ${VERSION}, older than this skill needs (${min_engine}). ` +
          "Tell the user to run `brew upgrade walkie` and restart Claude Code, then try again.")
      const root = checkRepo(repo)
      if (!NAME.test(name)) return fail(`name must be lower-case letters, digits and dashes: ${name}`)
      const probe = mkdtempSync(join(tmpdir(), "walkie-"))
      const resolved = await resolveProfile(root, probe, profile)
      rmSync(probe, { recursive: true, force: true })
      const work = join(root, resolved.profile.work_dir)
      const dir = join(work, name)
      const again = await resolveProfile(root, dir, profile)
      const spec = join(dir, again.profile.spec)
      if (!existsSync(spec)) cpSync(again.profile.template, spec)
      const flutter = again.profile.record.runner === "flutter"
      const notes = join(again.profile.dir, "NOTES.md")
      const ignored = (await run("git", ["-C", root, "check-ignore", "-q", work])).code === 0
      return text([
        `Engine: walkie ${VERSION}`,
        `Profile: ${again.profile.name} (${again.profile.chosen_because})`,
        `Walkie folder: ${dir}`,
        `Spec: ${spec}`,
        flutter
          ? `Runner: flutter (app ${join(root, again.profile.record.app_dir)}, device ${again.profile.record.device ?? "the running one"}). ` +
            "Read the walkie skill's references/flutter.md before writing the test."
          : "Runner: browser (Playwright)",
        `Voice: ${voiceLine(again.voice)}`,
        `Sequence: ${Object.entries(again.sequence).filter(([, v]) => typeof v === "boolean").map(([k, v]) => `${k} ${v ? "on" : "off"}`).join(", ")}`,
        ignored ? "" : `WARNING: ${work} is not gitignored; add it before recording.`,
        "",
        "Next: write the storyboard (one row per scene: action, element to highlight, caption, narration), " +
          "then call storyboard with it.",
        "",
        existsSync(notes) ? `--- ${again.profile.name} NOTES.md ---\n${readFileSync(notes, "utf8")}` : "",
      ].join("\n"))
    } catch (e) {
      return fail(String((e as Error).message ?? e))
    }
  })

  server.registerTool("storyboard", {
    title: "Write the storyboard and voice it",
    description:
      "Checkpoint 1. Save the scenes and their narration, generate the voice for every line, and return a " +
      "table with each line's length, pace and phonemes. Show the table to the user and wait for approval " +
      "before recording. Fix any line flagged FAST (Thai: add spaces where a person would breathe; English: " +
      "shorten it), CHOPPY (Thai: join phrases), UNKNOWN (English: words the voice cannot read and leaves out; add " +
      "a lexicon entry) or DROPPED (respell, or add a lexicon entry), and call again. Clip n0 is the intro; the last clip is the summary.",
    annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    inputSchema: z.object({
      repo: z.string().describe("Absolute path to the repo root"),
      name: z.string().describe("The walkie name given to start"),
      scenes: z.array(z.object({
        clip: z.string().regex(/^[a-z0-9]+$/).describe("Clip name used by demo.say(), e.g. n0, n1, n9"),
        caption: z.string().optional().describe("Caption as the UI spells it; <b> marks UI words"),
        highlight: z.string().optional().describe("The element to mark, in words or as a locator"),
        action: z.string().optional().describe("What happens on screen"),
        narration: z.string().describe("A sentence for the ear, in the profile's language. Thai: spaces are phrase breaks. {phonemes} allowed"),
      })).min(1),
    }),
  }, async ({ repo, name, scenes }) => {
    try {
      const dir = demoDir(checkRepo(repo), name)
      writeFileSync(join(dir, "storyboard.json"), JSON.stringify(scenes, null, 1))
      const tsv = scenes.map((s) => `${s.clip}\t${s.narration.replace(/[\t\n]+/g, " ")}`).join("\n") + "\n"
      writeFileSync(join(dir, "narration.tsv"), tsv)
      await refreshVoice(dir)
      const lines = await voice(dir, tsv)
      const byClip = Object.fromEntries(lines.map((l) => [l.clip, l]))
      const rows = scenes.map((s) => {
        const v = byClip[s.clip]
        return `| ${s.clip} | ${s.action ?? ""} | ${s.highlight ?? ""} | ${s.caption ?? ""} | ${s.narration} | ${v?.seconds ?? "?"}s | ${v?.rate ?? "?"}/s | ${v?.flags || "ok"} |`
      })
      const flagged = lines.filter((l) => l.flags)
      return text([
        "STOP: show this table to the user and wait for their approval before writing the spec or recording, " +
          "even when the request asked to go all the way to record. Approval of the storyboard is checkpoint 1.",
        "",
        "| clip | action | highlight | caption | narration | length | pace | check |",
        "|---|---|---|---|---|---|---|---|",
        ...rows,
        "",
        `Total voice: ${lines.reduce((t, l) => t + l.seconds, 0).toFixed(1)}s. Audio: ${join(dir, "voice")}/<clip>.wav`,
        flagged.length ? `Fix before recording: ${flagged.map((l) => `${l.clip} ${l.flags}`).join(", ")}` : "No flags.",
        "",
        "Phonemes (check each reads as meant; | is a phrase break):",
        ...lines.map((l) => `${l.clip}: ${l.phonemes}`),
        waitedNote(),
      ].filter((l, i, all) => l !== "" || i < all.length - 1).join("\n"))
    } catch (e) {
      return fail(String((e as Error).message ?? e))
    }
  })

  server.registerTool("record", {
    title: "Record, mix and check",
    description:
      "Checkpoint 2. Heavy work on this machine (recording, building, TTS) runs one step at a time: this " +
      "waits its turn behind other walkie work. Run demo.spec.ts with the engine's Playwright and Chromium (installed on first use; the app " +
      "must already be running, as NOTES.md says; base_url overrides the profile's record.base_url), " +
      "or, for a Flutter app, run demo_test.dart on the simulator or emulator (device overrides record.device) " +
      "while recording its screen; " +
      "mix the voice in, check for flashes, and copy the result to the library. Returns the MP4 and the " +
      "contact sheet: read the contact sheet image, check each clip shows its caption and highlight, show it " +
      "to the user and wait for approval. A failing expect can be a real bug in the change: report it.",
    annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: false },
    inputSchema: z.object({
      repo: z.string().describe("Absolute path to the repo root"),
      name: z.string().describe("The walkie name given to start"),
      pr: z.string().optional().describe("PR number or URL this video is for"),
      title: z.string().optional().describe("One line on what the video shows"),
      base_url: z.string().optional().describe("Where the app runs, when not at the profile's record.base_url (e.g. http://127.0.0.1:3110)"),
      device: z.string().optional().describe("Flutter: simulator or emulator name, iOS UDID or Android serial, when not the profile's record.device"),
    }),
  }, async ({ repo, name, pr, title, base_url, device }) => {
    try {
      const root = checkRepo(repo)
      const dir = demoDir(root, name)
      const profile = readJson(join(dir, "profile.json"))
      if (!existsSync(join(dir, "voice", "durations.json"))) return fail("no voice yet: call storyboard first")
      const results = join(dir, "results")
      rmSync(results, { recursive: true, force: true })
      rmSync(join(dir, "backdrops"), { recursive: true, force: true })
      await preflight()
      const flutter = profile.profile.record?.runner === "flutter"
      let raw: string
      let runDir: string | undefined
      if (flutter) {
        rmSync(join(dir, "raw.mp4"), { force: true })
        const rec = await heavy(`record ${videoId(root, name)}`, WT, ["device", "record", dir, root, ...(device ? [device] : [])],
          { cwd: root, timeoutMs: 20 * 60_000 })
        if (rec.code !== 0 || !existsSync(join(dir, "raw.mp4"))) {
          return fail(`Recording failed (exit ${rec.code}).\n${tail(rec.err || rec.out, 60)}\n\nTest output: ${join(dir, "test.log")}`)
        }
        raw = join(dir, "raw.mp4")
      } else {
        const rec = await heavy(`record ${videoId(root, name)}`, PLAYWRIGHT, ["test", "-c", join(SCRIPTS, "demo.config.ts")], {
          cwd: root,
          env: { DEMO_DIR: dir, ...(base_url ? { WALKIE_BASE_URL: base_url } : {}) },
        })
        runDir = existsSync(results) ? readdirSync(results).map((d) => join(results, d)).find((d) => existsSync(join(d, "video.webm"))) : undefined
        if (rec.code !== 0 || !runDir) {
          const shots = existsSync(results)
            ? readdirSync(results, { recursive: true }).map(String).filter((f) => f.endsWith(".png")).map((f) => join(results, f))
            : []
          const lines = (rec.out + "\n" + rec.err).split("\n")
          const first = lines.findIndex((l) => /Error|✘|Timed out/.test(l))
          const report = (first >= 0 ? lines.slice(first, first + 50) : lines.slice(-50)).join("\n")
          return fail(`Recording failed (exit ${rec.code}).\n${report}\n\nScreenshots:\n${shots.join("\n") || "none"}`)
        }
        raw = join(runDir, "video.webm")
      }
      // Layers -> video: page (content.mp4), overlay drawn from events.json, voice.
      const mp4 = join(dir, "walkie.mp4")
      const built = await heavy(`build ${videoId(root, name)}`, "node", ["--no-warnings", join(SCRIPTS, "build.ts"), dir, raw])
      if (built.code !== 0) return fail(`Building the video failed:\n${tail(built.err || built.out)}`)
      const contact = join(dir, "contact.png")
      const frames = await heavy(`check ${videoId(root, name)}`, WT, ["frames", mp4, dir, contact])
      const flashes = frames.out.split("\n").filter((l) => l.startsWith("FLASH"))
      const duration = parseFloat((await run("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", mp4])).out)

      const id = videoId(root, name)
      const lib = join(LIBRARY, id)
      rmSync(lib, { recursive: true, force: true })
      mkdirSync(lib, { recursive: true })
      cpSync(mp4, join(lib, "walkie.mp4"))
      cpSync(contact, join(lib, "contact.png"))
      if (runDir && existsSync(join(runDir, "trace.zip"))) cpSync(join(runDir, "trace.zip"), join(lib, "trace.zip"))
      // The layers, so the overlay (rerender) and the voice (revoice) can change without recording again.
      // stage.json: where a phone recording sits in the video, for rerender.
      for (const f of ["content.mp4", "events.json", "stage.json", "storyboard.json", "narration.tsv", "profile.json", "demo.spec.ts", "demo_test.dart"]) {
        if (existsSync(join(dir, f))) cpSync(join(dir, f), join(lib, f))
      }
      // The clips and their timing, so revoice can change one line later without recording again.
      mkdirSync(join(lib, "voice"), { recursive: true })
      for (const f of readdirSync(join(dir, "voice"))) {
        if (/\.(wav|json)$/.test(f) && !f.endsWith(".raw.wav")) cpSync(join(dir, "voice", f), join(lib, "voice", f))
      }
      // Stills of the page layer, one per step, for the configurator's preview backdrop.
      rmSync(join(lib, "backdrops"), { recursive: true, force: true })
      if (existsSync(join(dir, "backdrops"))) cpSync(join(dir, "backdrops"), join(lib, "backdrops"), { recursive: true })
      const manifest = {
        schema_version: 2, // schema/manifest.schema.json; 2 = layered (content.mp4 + events.json + voice/)
        id, name, title: title ?? null, pr: pr ?? null,
        repo: (await git(root, "remote", "get-url", "origin")) || root, repo_path: root,
        branch: await git(root, "rev-parse", "--abbrev-ref", "HEAD"),
        commit: await git(root, "rev-parse", "--short", "HEAD"),
        profile: profile.profile.name, created_at: new Date().toISOString(),
        duration_s: Math.round(duration * 10) / 10, flashes: flashes.length,
        clips: readJson(join(dir, "voice", "timeline.json")),
        usage: agentUsage(root, name, true), // the agent's tokens for this walkie, all sessions
        files: readdirSync(lib),
      }
      writeFileSync(join(lib, "manifest.json"), JSON.stringify(manifest, null, 1))
      rmSync(results, { recursive: true, force: true })
      rmSync(join(dir, "raw.mp4"), { force: true }) // content.mp4 is the page layer from here on
      return text([
        "STOP: read the contact sheet, then show it and the video path to the user and wait for approval " +
          "(checkpoint 2) before calling pr_text or anything else.",
        `Video: ${join(lib, "walkie.mp4")} (${manifest.duration_s}s)`,
        `Contact sheet: ${join(lib, "contact.png")} (two frames per clip)`,
        flutter ? `Test output: ${join(dir, "test.log")}` : `Trace: ${existsSync(join(lib, "trace.zip")) ? join(lib, "trace.zip") : "none"}`,
        flashes.length ? `FLASHES (fade these):\n${flashes.join("\n")}` : "No flashes.",
        `Library id: ${id}`,
        describeUsage(manifest.usage),
        waitedNote(),
      ].join("\n"))
    } catch (e) {
      return fail(String((e as Error).message ?? e))
    }
  })

  server.registerTool("voice_preview", {
    title: "Preview a voice line",
    description:
      "Voice one line with a profile, in its language, to test a spelling or a lexicon entry. Returns the WAV " +
      "path and the phonemes. {phonemes} inline work, e.g. Thai {sa↓.paːk↑}, English {ˈɛvəntpˌɑp}.",
    annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    inputSchema: z.object({
      text: z.string().describe("One line, as it would go in the narration; {phonemes} allowed"),
      repo: z.string().optional().describe("Repo whose profile to use; or give profile"),
      profile: z.string().optional().describe("Profile name; default when neither is given"),
    }),
  }, async ({ text: line, repo, profile }) => {
    try {
      const dir = join(PREVIEWS, String(Date.now()))
      mkdirSync(dir, { recursive: true })
      await resolveProfile(repo ? checkRepo(repo) : tmpdir(), dir, profile ?? (repo ? undefined : "default"))
      const [l] = await voice(dir, `preview\t${line.replace(/[\t\n]+/g, " ")}\n`)
      return text(`Audio: ${join(dir, "voice", "preview.wav")}\nLength: ${l.seconds}s, pace ${l.rate}/s ${l.flags}\nPhonemes: ${l.phonemes}`)
    } catch (e) {
      return fail(String((e as Error).message ?? e))
    }
  })

  server.registerTool("revoice", {
    title: "Change narration lines of a finished video",
    description:
      "Voice new text for some clips of a library video and mix it into the existing picture, without " +
      "recording again. Uses the profile's current voice settings and lexicon. A new clip that runs past " +
      "the start of the next one is not applied: shorten it, or record again. The previous MP4 is kept as " +
      "walkie.previous.mp4.",
    annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    inputSchema: z.object({
      id: z.string().describe("Library id, <repo>--<name>"),
      clips: z.array(z.object({
        clip: z.string().describe("Clip name, e.g. n2"),
        narration: z.string().describe("The new line, in the profile's language (Thai: spaces are phrase breaks)"),
      })).min(1).describe("Only the clips to change"),
    }),
  }, async ({ id, clips }) => {
    try {
      const lib = join(LIBRARY, id)
      if (!/^[\w.-]+--[a-z0-9-]+$/.test(id) || !existsSync(join(lib, "manifest.json"))) return fail(`no video ${id}`)
      if (!existsSync(join(lib, "voice", "timeline.json"))) return fail(`${id} was recorded before revoice existed (no voice/ in the library): record it again`)
      const manifest = readJson(join(lib, "manifest.json"))
      const timeline: { name: string; start: number }[] = readJson(join(lib, "voice", "timeline.json"))
      for (const c of clips) if (!timeline.some((t) => t.name === c.clip)) return fail(`no clip ${c.clip} in ${id}`)

      // The profile's voice as it is now (lexicon edits since the recording count), the rest as recorded.
      const prof = readJson(join(lib, "profile.json"))
      const now = await run(WT, ["profile", "--resolve", prof.profile.name])
      if (now.code === 0) prof.voice = JSON.parse(now.out).voice
      const work = mkdtempSync(join(tmpdir(), "revoice-"))
      writeFileSync(join(work, "profile.json"), JSON.stringify(prof, null, 1))
      const made = await voice(work, clips.map((c) => `${c.clip}\t${c.narration.replace(/[\t\n]+/g, " ")}\n`).join(""))

      const applied: string[] = []
      const refused: string[] = []
      for (const m of made) {
        const i = timeline.findIndex((t) => t.name === m.clip)
        const end = i + 1 < timeline.length ? timeline[i + 1].start : manifest.duration_s
        const room = end - timeline[i].start - 0.1
        if (m.seconds > room) {
          refused.push(`${m.clip}: ${m.seconds}s but only ${room.toFixed(2)}s before ${i + 1 < timeline.length ? timeline[i + 1].name : "the end"}`)
          continue
        }
        cpSync(join(work, "voice", `${m.clip}.wav`), join(lib, "voice", `${m.clip}.wav`))
        applied.push(m.clip)
      }
      if (applied.length) {
        const durations = readJson(join(lib, "voice", "durations.json"))
        for (const m of made) if (applied.includes(m.clip)) durations[m.clip] = m.seconds
        writeFileSync(join(lib, "voice", "durations.json"), JSON.stringify(durations, null, 1))
        const tsv = existsSync(join(lib, "narration.tsv")) ? readFileSync(join(lib, "narration.tsv"), "utf8").split("\n") : []
        for (const c of clips.filter((c) => applied.includes(c.clip))) {
          const line = `${c.clip}\t${c.narration}`
          const at = tsv.findIndex((l) => l.startsWith(`${c.clip}\t`))
          if (at >= 0) tsv[at] = line
          else tsv.push(line)
        }
        writeFileSync(join(lib, "narration.tsv"), tsv.join("\n"))
        const mp4 = join(lib, "walkie.mp4")
        const next = join(work, "walkie.mp4")
        const mix = await heavy(`revoice ${id}`, WT, ["mix", "remix", mp4, next, lib])
        if (mix.code !== 0) return fail(`Mixing failed:\n${tail(mix.err || mix.out)}`)
        cpSync(mp4, join(lib, "walkie.previous.mp4"))
        cpSync(next, mp4)
        manifest.revoiced_at = new Date().toISOString()
        manifest.files = readdirSync(lib)
        writeFileSync(join(lib, "manifest.json"), JSON.stringify(manifest, null, 1))
      }
      rmSync(work, { recursive: true, force: true })
      return text([
        applied.length ? `Revoiced ${applied.join(", ")}: ${join(lib, "walkie.mp4")} (previous: walkie.previous.mp4)` : "Nothing changed.",
        ...made.map((m) => `${m.clip}\t${m.seconds}s\t${m.rate}/s\t${m.flags || "ok"}\t${m.phonemes}`),
        refused.length ? `Too long, not applied (shorten, or record again):\n${refused.join("\n")}` : "",
        waitedNote(),
      ].filter(Boolean).join("\n"))
    } catch (e) {
      return fail(String((e as Error).message ?? e))
    }
  })



  server.registerTool("texts", {
    title: "List the text of a finished video",
    description: "The captions and cards of a library video, by id (step-1, caption-2, card-1 ...), as HTML. " +
      "Change them with rerender.",
    annotations: { readOnlyHint: true, openWorldHint: false },
    inputSchema: z.object({ id: z.string().describe("Library id, <repo>--<name>") }),
    outputSchema: z.object({
      items: z.array(z.object({ id: z.string(), t: z.number(), html: z.string() })).describe("In order of appearance; t in seconds"),
    }),
  }, async ({ id }) => {
    const lib = join(LIBRARY, id)
    if (!existsSync(join(lib, "events.json"))) return fail(`${id} has no overlay layer (recorded before layers): record it again`)
    const items = textItems(id).map((e: any) => ({ id: e.id as string, t: e.t as number, html: e.html as string }))
    return {
      content: [{ type: "text" as const, text: items.map((e: { id: string; t: number; html: string }) => `${e.id}\t${e.t}s\t${e.html.replace(/\s+/g, " ").trim()}`).join("\n") }],
      structuredContent: { items },
    }
  })

  server.registerTool("rerender", {
    title: "Draw a finished video's overlay again",
    description:
      "Rebuild a library video from its layers without recording again: the page layer stays, the overlay " +
      "is drawn again with the profile's current theme (colors, fonts, sizes, cards), and `text` replaces " +
      "captions or cards by id (see texts). Keep the step number span in a step caption. The previous MP4 " +
      "is kept as walkie.previous.mp4. Returns the new contact sheet: read it and show it to the user. " +
      "Timing does not change: a change to the sequence, the zoom, the video size or (Flutter) the phone's side " +
      "and margin needs a new recording; its frame radius, frame line and caption size can change here.",
    annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    inputSchema: z.object({
      id: z.string().describe("Library id, <repo>--<name>"),
      text: z.array(z.object({
        id: z.string().describe("Text id from texts, e.g. step-2, card-1"),
        html: z.string().describe("New HTML; keep a step caption's <span class=\"step\">N</span>"),
      })).optional().describe("Captions or cards to replace"),
      theme: z.enum(["current", "recorded"]).optional().describe("current (default): the profile's theme now"),
    }),
  }, async ({ id, text: edits = [], theme = "current" }) => {
    try {
      const lib = join(LIBRARY, id)
      if (!/^[\w.-]+--[a-z0-9-]+$/.test(id) || !existsSync(join(lib, "manifest.json"))) return fail(`no video ${id}`)
      if (!existsSync(join(lib, "content.mp4")) || !existsSync(join(lib, "events.json"))) {
        return fail(`${id} was recorded before layers existed (no content.mp4 / events.json): record it again`)
      }
      const rec = readJson(join(lib, "events.json"))
      const known = new Set(textItems(id).map((e: any) => e.id))
      const unknown = edits.filter((e) => !known.has(e.id)).map((e) => e.id)
      if (unknown.length) return fail(`no text ${unknown.join(", ")} in ${id}; ids: ${[...known].join(", ")}`)
      for (const edit of edits) for (const e of rec.events) if (e.id === edit.id) e.html = edit.html

      const prof = readJson(join(lib, "profile.json"))
      if (theme === "current") {
        const now = await run(WT, ["profile", "--resolve", prof.profile.name])
        if (now.code !== 0) return fail(`profile ${prof.profile.name}: ${tail(now.err)}`)
        const cur = JSON.parse(now.out)
        // Only the look: layout fields that move the page (zoom, video size, where a phone sits)
        // need a new recording.
        const { video, zoom, device } = prof.theme.layout
        const keep = { ...cur.theme.layout.device, side: device?.side, margin: device?.margin, zoom_scale: device?.zoom_scale }
        prof.theme = { ...cur.theme, layout: { ...cur.theme.layout, video, zoom, ...(device ? { device: keep } : {}) } }
      }
      const work = mkdtempSync(join(tmpdir(), "rerender-"))
      writeFileSync(join(work, "profile.json"), JSON.stringify(prof, null, 1))
      writeFileSync(join(work, "events.json"), JSON.stringify(rec, null, 1))
      cpSync(join(lib, "content.mp4"), join(work, "content.mp4"))
      if (existsSync(join(lib, "stage.json"))) cpSync(join(lib, "stage.json"), join(work, "stage.json"))
      cpSync(join(lib, "voice"), join(work, "voice"), { recursive: true })
      const built = await heavy(`rerender ${id}`, "node", ["--no-warnings", join(SCRIPTS, "build.ts"), work])
      if (built.code !== 0) return fail(`Building the video failed:\n${tail(built.err || built.out)}`)
      const frames = await heavy(`check ${id}`, WT, ["frames", join(work, "walkie.mp4"), work, join(work, "contact.png")])
      const flashes = frames.out.split("\n").filter((l) => l.startsWith("FLASH"))

      cpSync(join(lib, "walkie.mp4"), join(lib, "walkie.previous.mp4"))
      for (const f of ["walkie.mp4", "contact.png", "events.json", "profile.json"]) cpSync(join(work, f), join(lib, f))
      rmSync(work, { recursive: true, force: true })
      const manifest = readJson(join(lib, "manifest.json"))
      Object.assign(manifest, { rerendered_at: new Date().toISOString(), flashes: flashes.length, files: readdirSync(lib) })
      writeFileSync(join(lib, "manifest.json"), JSON.stringify(manifest, null, 1))
      return text([
        `Rerendered ${id} (${theme} theme${edits.length ? `, ${edits.length} text change${edits.length > 1 ? "s" : ""}` : ""}).`,
        `Video: ${join(lib, "walkie.mp4")} (previous: walkie.previous.mp4)`,
        `Contact sheet: ${join(lib, "contact.png")}: read it and show it to the user.`,
        flashes.length ? `FLASHES (fade these):\n${flashes.join("\n")}` : "No flashes.",
        waitedNote(),
      ].filter(Boolean).join("\n"))
    } catch (e) {
      return fail(String((e as Error).message ?? e))
    }
  })

  server.registerTool("lexicon", {
    title: "Read or edit a profile's lexicon",
    description:
      "The lexicon maps a word, as written in the narration, to exact phonemes when the TTS reads it wrong. " +
      "Thai: tones ↓ low, → mid, ↑ high, ↗ rising, ↘ falling; . between syllables (สปาร์ก -> sa↓.paːk↑). English: " +
      "misaki's US set, ˈ and ˌ stress before the vowel, A I O W Y for the diphthongs of day, my, go, now, boy " +
      "(Eventpop -> ˈɛvəntpˌɑp). Preview with voice_preview first.",
    annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    inputSchema: z.object({
      profile: z.string().describe("Profile name, from profiles"),
      set: z.record(z.string(), z.string()).optional().describe("word -> phonemes to add or change"),
      remove: z.array(z.string()).optional().describe("Words to drop from the lexicon"),
    }),
  }, async ({ profile, set, remove }) => {
    const dir = (await profileDirs())[profile]
    if (!dir) return fail(`no profile ${profile}`)
    const file = join(dir, "voice.json")
    const v = existsSync(file) ? readJson(file) : {}
    v.lexicon = { ...(v.lexicon ?? {}), ...(set ?? {}) }
    for (const w of remove ?? []) delete v.lexicon[w]
    if (set || remove) writeFileSync(file, JSON.stringify(v, null, 2) + "\n")
    const lines = Object.entries(v.lexicon).map(([w, p]) => `${w}\t${p}`)
    return text(`${profile} lexicon (${lines.length}):\n${lines.join("\n") || "(empty)"}`)
  })

  server.registerTool("profiles", {
    title: "List profiles",
    description: "Profiles with the repos they match and allow, and where each lives. Profiles are found in WALKIE_PROFILES, ~/.config/walkie/profiles, then the built-in profiles/.",
    annotations: { readOnlyHint: true, openWorldHint: false },
    inputSchema: z.object({}),
    outputSchema: z.object({
      profiles: z.array(z.object({
        name: z.string(), dir: z.string(), match: z.array(z.string()), allow: z.array(z.string()).nullable(),
      })),
    }),
  }, async () => {
    const profiles = Object.entries(await profileDirs()).map(([name, dir]) => {
      const d = readJson(join(dir, "profile.json"))
      return { name, dir, match: (d.match ?? []) as string[], allow: (d.allow ?? null) as string[] | null }
    })
    return {
      content: [{ type: "text" as const, text: profiles.map((p) =>
        `${p.name}: match ${JSON.stringify(p.match)}${p.allow ? `, allow ${JSON.stringify(p.allow)}` : ""}\n  ${p.dir}`).join("\n") }],
      structuredContent: { profiles },
    }
  })



  server.registerTool("pr_text", {
    title: "PR description text for a video",
    description:
      "Write the Markdown for a PR description or comment from a video in the library: title, length, " +
      "branch and commit, and a table of the steps. Copies it to the clipboard (macOS) unless copy is false. " +
      "The text marks where to drop the MP4: GitHub only takes video attachments dragged in the browser.",
    annotations: { readOnlyHint: true, openWorldHint: false },
    inputSchema: z.object({
      id: z.string().describe("Library id, from record or list"),
      copy: z.boolean().default(true).describe("Copy the Markdown to the clipboard (macOS)"),
    }),
  }, async ({ id, copy }) => {
    const dir = join(LIBRARY, id)
    if (!existsSync(join(dir, "manifest.json"))) return fail(`no video ${id}`)
    const m = readJson(join(dir, "manifest.json"))
    const scenes = existsSync(join(dir, "storyboard.json")) ? readJson(join(dir, "storyboard.json")) : []
    const md = prText(m, scenes)
    let note = "Not copied."
    if (copy) {
      const r = await run("pbcopy", [], { input: md })
      note = r.code === 0 ? "Copied to the clipboard." : "Could not copy (pbcopy unavailable); use the text below."
    }
    return text(`${note} Video file: ${join(dir, "walkie.mp4")}\n\n${md}`)
  })

  server.registerTool("list", {
    title: "List videos",
    description: "Videos in the library on this machine, newest first.",
    annotations: { readOnlyHint: true, openWorldHint: false },
    inputSchema: z.object({ repo: z.string().optional().describe("Only this repo (path or name)") }),
    outputSchema: z.object({
      videos: z.array(z.object({
        id: z.string(), title: z.string().nullable(), pr: z.string().nullable(), repo_path: z.string(),
        branch: z.string(), commit: z.string(), created_at: z.string(), duration_s: z.number(),
        video: z.string().describe("Path of the MP4"),
        layered: z.boolean().describe("true: rerender and revoice work (schema_version 2)"),
        usage: z.unknown().describe("Agent tokens (manifest usage), or null"),
      })).describe("Newest first"),
    }),
  }, async ({ repo }) => {
    const all = listLibrary().filter((m) => !repo || m.repo_path === resolve(repo) || m.id.startsWith(`${basename(repo)}--`))
    const videos = all.map((m) => ({
      id: m.id, title: m.title ?? null, pr: m.pr ?? null, repo_path: m.repo_path, branch: m.branch, commit: m.commit,
      created_at: m.created_at, duration_s: m.duration_s, video: join(LIBRARY, m.id, "walkie.mp4"),
      layered: m.schema_version >= 2, usage: m.usage ?? null,
    }))
    const lines = all.map((m) =>
      `${m.id}  ${m.duration_s}s  ${new Date(m.created_at).toLocaleString("sv-SE").slice(0, 16)}  ${m.branch}@${m.commit}${m.pr ? `  PR ${m.pr}` : ""}${m.title ? `  ${m.title}` : ""}\n  ${join(LIBRARY, m.id, "walkie.mp4")}\n  ${describeUsage(m.usage)}`)
    return {
      content: [{ type: "text" as const, text: lines.join("\n") || `No videos in ${LIBRARY}` }],
      structuredContent: { videos },
    }
  })

  server.registerTool("delete", {
    title: "Delete a video",
    description: "Delete one video from the library (the MP4, contact sheet, trace and manifest). Ask the user first.",
    annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: true, openWorldHint: false },
    inputSchema: z.object({ id: z.string().describe("Library id, <repo>--<name>, from list") }),
  }, async ({ id }) => {
    const dir = join(LIBRARY, id)
    if (!/^[\w.-]+--[a-z0-9-]+$/.test(id) || !existsSync(join(dir, "manifest.json"))) return fail(`no video ${id}`)
    rmSync(dir, { recursive: true, force: true })
    return text(`Deleted ${id}`)
  })

  server.registerTool("purge", {
    title: "Purge old videos",
    description:
      "Delete library videos older than a number of days, and old voice previews. Runs as a dry run unless " +
      "confirm is true: show the user the list first.",
    annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: true, openWorldHint: false },
    inputSchema: z.object({
      older_than_days: z.number().int().min(0).describe("Delete videos and previews older than this many days"),
      confirm: z.boolean().default(false).describe("false: list what would go (dry run); true: delete"),
    }),
  }, async ({ older_than_days, confirm }) => {
    const cutoff = Date.now() - older_than_days * 86_400_000
    const old = listLibrary().filter((m) => Date.parse(m.created_at) < cutoff)
    const previews = PREVIEWS
    const oldPreviews = existsSync(previews)
      ? readdirSync(previews).filter((p) => statSync(join(previews, p)).mtimeMs < cutoff)
      : []
    if (confirm) {
      for (const m of old) rmSync(join(LIBRARY, m.id), { recursive: true, force: true })
      for (const p of oldPreviews) rmSync(join(previews, p), { recursive: true, force: true })
    }
    return text(`${confirm ? "Deleted" : "Would delete"} ${old.length} video(s) and ${oldPreviews.length} preview(s):\n` +
      (old.map((m) => `${m.id}  ${m.created_at.slice(0, 10)}`).join("\n") || "(no videos)"))
  })

  return server
}

serveStdio(createServer, { onerror: (error) => console.error("walkie MCP server:", error) })
