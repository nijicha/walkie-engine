// One recording at a time on this machine. Two Chromiums recording at once fight for the CPU and
// the video stutters, so every recording takes a ticket and waits for the tickets before it.
//
// A ticket is a file in the queue folder, named so that names sort by arrival. It is your turn when
// no earlier ticket is left. Tickets of dead processes, or older than MAX_AGE, are dropped, so a
// crash never blocks the queue.
//
// Library: await inQueue("shop--checkout-123", () => record(), (ahead) => log(ahead))
// CLI:     node scripts/queue.ts <label> -- <command> [args...]
import { spawn } from "node:child_process"
import { mkdirSync, readdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { QUEUE } from "./paths.ts"

export { QUEUE }
const MAX_AGE = 2 * 60 * 60 * 1000
const POLL = 1000

type Ticket = { label: string; pid: number; since: string }

function alive(pid: number) {
  try {
    process.kill(pid, 0)
    return true
  } catch (e) {
    return (e as NodeJS.ErrnoException).code === "EPERM"
  }
}

// The live tickets that came before `mine`, oldest first. Drops dead ones on the way.
function ahead(mine: string): Ticket[] {
  const out: Ticket[] = []
  for (const name of readdirSync(QUEUE).sort()) {
    if (name >= mine) break
    const file = join(QUEUE, name)
    try {
      const t: Ticket = JSON.parse(readFileSync(file, "utf8"))
      if (alive(t.pid) && Date.now() - Date.parse(t.since) < MAX_AGE) out.push(t)
      else unlinkSync(file)
    } catch {
      // a ticket being written or removed right now: look again next poll
    }
  }
  return out
}

export async function inQueue<T>(
  label: string,
  work: () => Promise<T>,
  onWait?: (ahead: Ticket[]) => void,
): Promise<{ result: T; waitedMs: number }> {
  mkdirSync(QUEUE, { recursive: true })
  const name = `${String(Date.now()).padStart(15, "0")}-${process.pid}-${Math.random().toString(36).slice(2, 8)}`
  const file = join(QUEUE, name)
  writeFileSync(file, JSON.stringify({ label, pid: process.pid, since: new Date().toISOString() } satisfies Ticket))
  const start = Date.now()
  try {
    let last = ""
    for (;;) {
      const before = ahead(name)
      if (!before.length) break
      const now = before.map((t) => t.label).join(", ")
      if (now !== last) onWait?.(before)
      last = now
      await new Promise((done) => setTimeout(done, POLL))
    }
    return { result: await work(), waitedMs: Date.now() - start }
  } finally {
    try {
      unlinkSync(file)
    } catch {}
  }
}

// CLI wrapper, for scripts: wait for a turn, run the command, pass its exit code on.
if (import.meta.url === `file://${process.argv[1]}`) {
  const [label, dashes, cmd, ...args] = process.argv.slice(2)
  if (!label || dashes !== "--" || !cmd) {
    console.error("usage: node queue.ts <label> -- <command> [args...]")
    process.exit(2)
  }
  const { result } = await inQueue(label, () => new Promise<number>((done) => {
    spawn(cmd, args, { stdio: "inherit" }).on("close", (code) => done(code ?? 1))
  }), (before) => console.error(`waiting for: ${before.map((t) => t.label).join(", ")}`))
  process.exit(result)
}
