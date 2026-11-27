// Tokens the agent spent making one walkie, read from its Claude Code transcript.
//
// Claude Code logs every API call, with its token usage, in ~/.claude/projects/<dir>/<session>.jsonl
// (CLAUDE_CONFIG_DIR moves it). In each session that worked on the walkie, its window opens at
// the user prompt that led to the session's first walkie call for it and closes at the last one
// (or now, while recording). Every API call in a window counts, subagents included, so retries and
// re-records count too. The walkie subagent's own log counts the same way; the main session's
// turns that brief it and relay its checkpoints do not (they call no walkie tool).
// Other agents (Codex, ...) keep no such log here: the result is then null.
//
// Library: agentUsage(repoPath, name)                  -> Usage | null
// CLI:     node scripts/usage.ts <repo path> <name>    (prints JSON)
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"

export type Usage = {
  sessions: string[]
  from: string
  to: string
  calls: number
  models: string[]
  input: number // uncached input tokens
  cache_write: number
  cache_read: number
  output: number
}

const PROJECTS = join(process.env.CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude"), "projects")
const RECENT = 14 * 24 * 60 * 60 * 1000

type Entry = { type?: string; timestamp?: string; message?: any }

function entries(file: string): Entry[] {
  const out: Entry[] = []
  for (const line of readFileSync(file, "utf8").split("\n")) {
    if (!line) continue
    try {
      out.push(JSON.parse(line))
    } catch {}
  }
  return out
}

const toolUses = (e: Entry) =>
  Array.isArray(e.message?.content) ? e.message.content.filter((c: any) => c?.type === "tool_use") : []

// A prompt the user typed, not a tool result coming back.
function isPrompt(e: Entry) {
  if (e.type !== "user" || e.message?.role !== "user") return false
  const c = e.message.content
  return typeof c === "string" || (Array.isArray(c) && c.some((p: any) => p?.type === "text"))
}

function transcripts(): string[] {
  if (!existsSync(PROJECTS)) return []
  const out: string[] = []
  for (const dir of readdirSync(PROJECTS)) {
    const d = join(PROJECTS, dir)
    let files: string[]
    try {
      files = readdirSync(d).filter((f) => f.endsWith(".jsonl"))
    } catch {
      continue
    }
    for (const f of files) {
      const p = join(d, f)
      if (Date.now() - statSync(p).mtimeMs < RECENT) out.push(p)
      // A session's subagents log on their own: the walkie subagent makes its calls there.
      const subs = join(d, f.replace(/\.jsonl$/, ""), "subagents")
      if (existsSync(subs)) {
        for (const a of readdirSync(subs)) if (a.endsWith(".jsonl") && Date.now() - statSync(join(subs, a)).mtimeMs < RECENT) out.push(join(subs, a))
      }
    }
  }
  return out
}

// Calls to any walkie tool for this repo and name (list and pr_text name it by id, so they don't count).
function isWalkieCall(e: Entry, repo: string, name: string) {
  return toolUses(e).some((c: any) => /walkie__\w+$/.test(c.name ?? "") && c.input?.name === name && c.input?.repo === repo)
}

export function agentUsage(repo: string, name: string, untilNow = false): Usage | null {
  const calls = new Map<string, any>()
  const models = new Set<string>()
  const sessions: string[] = []
  let first = Infinity
  let last = 0
  for (const file of transcripts()) {
    const text = readFileSync(file, "utf8")
    if (!text.includes("walkie__") || !text.includes(name)) continue
    const log = entries(file)
    const firstCall = log.findIndex((e) => isWalkieCall(e, repo, name))
    if (firstCall < 0) continue
    sessions.push(file.replace(PROJECTS + "/", "").replace(/\.jsonl$/, "").split("/").slice(1).filter((p) => p !== "subagents").join("/"))

    // This session's window: from the prompt that led to its first call to its last call.
    let open = firstCall
    while (open > 0 && !isPrompt(log[open])) open--
    const from = Date.parse(log[open].timestamp ?? log[firstCall].timestamp!)
    const lastCall = log.findLastIndex((e) => isWalkieCall(e, repo, name))
    const to = untilNow ? Date.now() : Date.parse(log[lastCall].timestamp!)
    first = Math.min(first, from)
    last = Math.max(last, to)

    // The session's own log plus its subagents' logs, cut to the window.
    const subDir = join(file.replace(/\.jsonl$/, ""), "subagents")
    const subs = existsSync(subDir) ? readdirSync(subDir).filter((f) => f.endsWith(".jsonl")).map((f) => entries(join(subDir, f))) : []
    for (const l of [log, ...subs]) {
      for (const e of l) {
        const u = e.message?.usage
        const t = Date.parse(e.timestamp ?? "")
        if (!u || !(t >= from && t <= to)) continue
        // One API call can span several log lines that repeat its usage: count each message id once.
        calls.set(e.message.id ?? `${t}`, u)
        if (e.message.model && e.message.model !== "<synthetic>") models.add(e.message.model)
      }
    }
  }
  if (!sessions.length) return null
  const sum = (k: string) => [...calls.values()].reduce((n, u) => n + (u[k] ?? 0), 0)
  return {
    sessions,
    from: new Date(first).toISOString(),
    to: new Date(last).toISOString(),
    calls: calls.size,
    models: [...models],
    input: sum("input_tokens"),
    cache_write: sum("cache_creation_input_tokens"),
    cache_read: sum("cache_read_input_tokens"),
    output: sum("output_tokens"),
  }
}

const k = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : `${n}`)

/** One line for people: "412k output, 1.2M new input, 38.5M cached reads over 233 calls (claude-opus-5-5)". */
export function describeUsage(u: Usage | null | undefined): string {
  if (!u) return "Agent tokens: unknown (no Claude Code transcript for this walkie)."
  return `Agent tokens: ${k(u.output)} output, ${k(u.input + u.cache_write)} new input, ${k(u.cache_read)} cached reads ` +
    `over ${u.calls} calls (${u.models.join(", ") || "unknown model"}).`
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [repo, name] = process.argv.slice(2)
  if (!repo || !name) {
    console.error("usage: node scripts/usage.ts <repo path> <name>")
    process.exit(2)
  }
  const u = agentUsage(repo, name)
  console.log(JSON.stringify(u, null, 1))
  console.log(describeUsage(u))
}
