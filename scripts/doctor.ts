// walkie doctor: checks what walkie needs on this Mac and says how to fix what is missing.
// The report is meant to be pasted into a GitHub issue: the home folder is shown as ~, and it
// holds no repo names, profile contents or narration.
//
// Run: walkie doctor (exits 1 when a check fails; warnings do not fail it)
import { execFileSync } from "node:child_process"
import { accessSync, constants, existsSync, readdirSync, readFileSync, statfsSync } from "node:fs"
import { arch, homedir, release } from "node:os"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { DATA, LIBRARY, QUEUE, WT } from "./paths.ts"
import { compareVersions } from "./version.ts"

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..")
const VERSION: string = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).version
const NODE_MIN = "23.6.0"

type Level = "ok" | "warn" | "FAIL"
let failed = 0, warned = 0
const tilde = (s: string) => s.split(homedir()).join("~")
function report(level: Level, what: string, detail: string, fix = "") {
  if (level === "FAIL") failed++
  if (level === "warn") warned++
  console.log(`${level.padEnd(4)}  ${what.padEnd(12)} ${tilde(detail)}`)
  if (fix && level !== "ok") console.log(`${" ".repeat(19)}fix: ${tilde(fix)}`)
}
function out(cmd: string, args: string[], opts: { timeout?: number } = {}): string | null {
  try {
    return execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: opts.timeout ?? 30_000 }).trim()
  } catch {
    return null
  }
}
const gb = (bytes: number) => `${(bytes / 1024 ** 3).toFixed(1)} GB`

console.log(`walkie doctor (engine ${VERSION})\n`)

// Engine and machine
const brew = ROOT.includes("/Cellar/walkie/")
report("ok", "engine", `walkie ${VERSION}, ${brew ? "installed with Homebrew" : "a source checkout"} (${ROOT})`)
const macos = out("sw_vers", ["-productVersion"])
report(arch() === "arm64" ? "ok" : "FAIL", "mac", `macOS ${macos ?? `(darwin ${release()})`}, ${arch()}`,
  "walkie needs an Apple Silicon Mac; Intel is not supported yet")
const node = process.versions.node
report(compareVersions(node, NODE_MIN) >= 0 ? "ok" : "FAIL", "node", `${node} (needs ${NODE_MIN} or newer)`,
  brew ? "brew upgrade node" : "install Node 23.6+ (mise use node@latest)")
const ffmpeg = out("ffmpeg", ["-version"])?.match(/^ffmpeg version \S+/)?.[0]
report(ffmpeg ? "ok" : "FAIL", "ffmpeg", ffmpeg ?? "not found on PATH", "brew install ffmpeg")

// What walkie setup installs
const playwright = join(ROOT, "node_modules", ".bin", "playwright")
if (!existsSync(playwright)) {
  report("FAIL", "packages", "the engine's Node packages are missing", "walkie setup")
} else {
  report("ok", "packages", `Playwright ${out(playwright, ["--version"])?.replace(/^Version /, "") ?? "?"}`)
  const where = out(playwright, ["install", "--dry-run", "chromium"])?.match(/Install location:\s+(.+)/)?.[1]
  report(where && existsSync(where) ? "ok" : "FAIL", "chromium", where && existsSync(where) ? where : "the Chromium this Playwright records with is missing", "walkie setup")
}
const wt = existsSync(WT) ? out(WT, ["tts", "--installed"]) : null
if (wt === null) {
  report("FAIL", "tools", "the engine's tools (bin/wt) are missing or do not run", brew ? "brew reinstall walkie" : "walkie setup (builds them with Rust from mise)")
} else {
  report("ok", "tools", "bin/wt runs")
  const voices = wt.split(/\s+/).filter(Boolean)
  const names: Record<string, string> = { th: "Thai (MakHam)", en: "English (Kokoro)" }
  report(voices.length ? "ok" : "FAIL", "voices", voices.length ? voices.map((v) => names[v] ?? v).join(", ") : "no voice is set up",
    "walkie setup (walkie setup list shows the voices)")
  if (voices.length && !voices.includes("th")) report("warn", "", "no Thai voice: the default profile and walkie smoke speak Thai", "walkie setup th")
}

// Profiles: names only (a profile's contents can be private to a team)
const profiles = existsSync(WT) ? out(WT, ["profile", "--list"]) : null
if (profiles === null) report("FAIL", "profiles", "could not list the profiles", "walkie profile --list shows the error")
else {
  const names = Object.keys(JSON.parse(profiles))
  report("ok", "profiles", `${names.length}: ${names.join(", ")}`)
}

// Data folders
try {
  if (existsSync(DATA)) accessSync(DATA, constants.W_OK)
  const videos = existsSync(LIBRARY) ? readdirSync(LIBRARY).filter((d) => existsSync(join(LIBRARY, d, "manifest.json"))).length : 0
  report("ok", "library", `${LIBRARY}, ${videos} video${videos === 1 ? "" : "s"}`)
} catch {
  report("FAIL", "library", `${DATA} is not writable`, `check the folder's owner: ls -ld "${tilde(DATA)}"`)
}
const tickets = existsSync(QUEUE) ? readdirSync(QUEUE).length : 0
report(tickets > 2 ? "warn" : "ok", "queue", tickets ? `${tickets} job${tickets === 1 ? "" : "s"} waiting or running` : "empty",
  "another walkie is recording; tickets of stopped jobs are dropped on their own after 2 hours")
const fs = statfsSync(existsSync(DATA) ? DATA : homedir())
const free = fs.bavail * fs.bsize
report(free > 2 * 1024 ** 3 ? "ok" : "warn", "disk", `${gb(free)} free`, "a recording needs about 1 GB while it builds: walkie purge or free some space")

// Claude Code: the plugin is how most people use walkie
const claude = out("claude", ["--version"])
if (!claude) report("warn", "claude code", "not found on PATH", "install Claude Code, then: /plugin install walkie --marketplace nijicha/skills")
else {
  const plugins = out("claude", ["plugin", "list"], { timeout: 20_000 })
  const line = plugins?.split("\n").find((l) => /walkie@/.test(l))
  const version = plugins?.split("\n").slice(plugins.split("\n").indexOf(line ?? "")).find((l) => /Version:/.test(l))?.replace(/.*Version:\s*/, "")
  report(line ? "ok" : "warn", "claude code", `${claude.split(" ")[0]}, plugin walkie ${line ? (version ?? "installed") : "not installed"}`,
    "/plugin install walkie --marketplace nijicha/skills")
}

console.log(`\n${failed ? `${failed} failed` : "All checks passed"}${warned ? `, ${warned} warning${warned === 1 ? "" : "s"}` : ""}.`)
process.exit(failed ? 1 : 0)
