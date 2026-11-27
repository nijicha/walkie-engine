// End-to-end check of the MCP server: node tests/mcp/client.ts <repo>
// Uses the smoke page and spec, so no app has to run.
import { Client } from "@modelcontextprotocol/client"
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio"
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const SKILL = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const repo = resolve(process.argv[2] ?? SKILL)  // this repo itself
// An English profile, next to the user's own: only its voice language and a lexicon entry.
const profiles = mkdtempSync(join(tmpdir(), "walkie-profiles-"))
mkdirSync(join(profiles, "en-check"))
writeFileSync(join(profiles, "en-check", "profile.json"), JSON.stringify({ name: "en-check" }))
writeFileSync(join(profiles, "en-check", "voice.json"), JSON.stringify({ language: "en", lexicon: { Eventpop: "ˈɛvəntpˌɑp" } }))
const env = {
  ...process.env,
  WALKIE_HOME: process.env.WALKIE_HOME ?? "/tmp/walkie-test/videos",
  WALKIE_PROFILES: [profiles, process.env.WALKIE_PROFILES].filter(Boolean).join(":"),
} as Record<string, string>
const client = new Client({ name: "walkie-test", version: "0" })
await client.connect(new StdioClientTransport({ command: join(SKILL, "bin/walkie"), args: ["mcp"], env }))

// expectError: the call must fail (a check of the server's refusals); anything else must succeed.
const call = async (name: string, args: Record<string, unknown> = {}, { expectError = false } = {}) => {
  const r: any = await client.callTool({ name, arguments: args })
  const out = r.content.map((c: any) => c.text).join("\n")
  console.log(`\n=== ${name}${r.isError ? " (ERROR)" : ""}\n${out}`)
  if (!!r.isError !== expectError) {
    console.error(`FAIL: ${name} ${expectError ? "should have failed" : "failed"}`)
    process.exit(1)
  }
  if (r.structuredContent) console.log(`structured: ${Object.keys(r.structuredContent).join(", ")}`)
  return out
}

console.log("tools:", (await client.listTools()).tools.map((t) => t.name).join(", "))
await call("profiles")
await call("start", { repo, name: "mcp-smoke", min_engine: "99.0.0" }, { expectError: true })
const started = await call("start", { repo, name: "mcp-smoke", min_engine: "0.1.0-alpha" })
if (!started.includes("Engine: walkie ")) { console.error("FAIL: start does not report the engine version"); process.exit(1) }
if (!started.includes("Voice: Thai")) { console.error("FAIL: start does not say the narration language"); process.exit(1) }
const dir = started.match(/Walkie folder: (.+)/)![1]
for (const f of ["demo.spec.ts", "page.html", "dark.html"]) cpSync(join(SKILL, "tests/smoke", f), join(dir, f))
for (const f of ["Prompt-Medium.woff2", "Prompt-Bold.woff2"]) cpSync(join(SKILL, "profiles/default/fonts", f), join(dir, f))
const lines = readFileSync(join(SKILL, "tests/smoke/narration.tsv"), "utf8").trim().split("\n").map((l) => l.split("\t"))
const captions: Record<string, string> = { n1: "กรอกอีเมล แล้วกด <b>“บันทึก”</b>", n2: "หน้าที่มีตัวหนังสือเยอะ ใช้ <b>spotlight</b>", n3: "หน้าที่ข้อมูลเยอะ ใช้ <b>zoom</b>" }
await call("storyboard", { repo, name: "mcp-smoke", scenes: lines.map(([clip, narration]) => ({ clip, narration, caption: captions[clip] })) })
await call("voice_preview", { text: "ทดสอบ สปาร์ก", repo })
// English: the lexicon entry is read; a word the G2P does not know is flagged.
const en = await call("voice_preview", { text: "Open Eventpop, then press Save at 10:30.", profile: "en-check" })
if (en.includes("UNKNOWN") || !en.includes("ˈɛvəntpˌɑp")) { console.error("FAIL: English preview did not use the lexicon"); process.exit(1) }
const unknown = await call("voice_preview", { text: "Built with Kokoro.", profile: "en-check" })
if (!unknown.includes("UNKNOWN Kokoro")) { console.error("FAIL: English preview did not flag an unknown word"); process.exit(1) }
// A lexicon entry added after start counts in the next storyboard run (the walkie's profile.json is
// resolved at start).
const enStart = await call("start", { repo, name: "mcp-smoke-en", profile: "en-check", min_engine: "0.2.0-alpha" })
if (!enStart.includes("Voice: English")) { console.error("FAIL: start does not say English"); process.exit(1) }
const enScenes = [{ clip: "n0", narration: "Built with Kokoro." }]
if (!(await call("storyboard", { repo, name: "mcp-smoke-en", scenes: enScenes })).includes("UNKNOWN Kokoro")) { console.error("FAIL: no UNKNOWN before the lexicon entry"); process.exit(1) }
await call("lexicon", { profile: "en-check", set: { Kokoro: "kəkˈɔɹO" } })
if ((await call("storyboard", { repo, name: "mcp-smoke-en", scenes: enScenes })).includes("UNKNOWN")) { console.error("FAIL: storyboard ignored a lexicon entry added after start"); process.exit(1) }
await call("record", { repo, name: "mcp-smoke", title: "MCP smoke test" })
// One line that fits its gap, one far too long for it (must be refused).
await call("revoice", { id: `${repo.split("/").pop()}--mcp-smoke`, clips: [
  { clip: "n1", narration: "กรอกอีเมล แล้วกดบันทึก" },
  { clip: "n2", narration: "หน้านี้ตัวหนังสือเยอะมาก จึงหรี่ส่วนอื่นลง ให้คนดูอ่านเฉพาะส่วนที่สำคัญ แล้วค่อยไปต่อที่ขั้นตอนถัดไป อย่างช้าๆ ทีละขั้น" },
] })
await call("texts", { id: `${repo.split("/").pop()}--mcp-smoke` })
await call("rerender", { id: `${repo.split("/").pop()}--mcp-smoke`, text: [{ id: "step-1", html: '<span class="step">1</span>แก้ข้อความ <b>หลังอัด</b>' }] })
await call("rerender", { id: `${repo.split("/").pop()}--mcp-smoke`, text: [{ id: "step-99", html: "x" }] }, { expectError: true })
await call("list")
const pr = await call("pr_text", { id: `${repo.split("/").pop()}--mcp-smoke` })
if (!pr.includes("Made with [Walkie]")) { console.error("FAIL: pr_text has no Made with Walkie line"); process.exit(1) }
await call("purge", { older_than_days: 0 })
await call("delete", { id: `${repo.split("/").pop()}--mcp-smoke` })
await client.close()
console.log("\nALL PASSED")
