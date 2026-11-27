// End-to-end check of the MCP server: node tests/mcp/client.ts <repo>
// Uses the smoke page and spec, so no app has to run.
import { Client } from "@modelcontextprotocol/client"
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio"
import { cpSync, readFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const SKILL = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const repo = resolve(process.argv[2] ?? SKILL)  // this repo itself
const env = { ...process.env, WALKIE_HOME: process.env.WALKIE_HOME ?? "/tmp/walkie-test/videos" } as Record<string, string>
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
const started = await call("start", { repo, name: "mcp-smoke" })
const dir = started.match(/Walkie folder: (.+)/)![1]
for (const f of ["demo.spec.ts", "page.html", "dark.html"]) cpSync(join(SKILL, "tests/smoke", f), join(dir, f))
for (const f of ["Prompt-Medium.woff2", "Prompt-Bold.woff2"]) cpSync(join(SKILL, "profiles/default/fonts", f), join(dir, f))
const lines = readFileSync(join(SKILL, "tests/smoke/narration.tsv"), "utf8").trim().split("\n").map((l) => l.split("\t"))
const captions: Record<string, string> = { n1: "กรอกอีเมล แล้วกด <b>“บันทึก”</b>", n2: "หน้าที่มีตัวหนังสือเยอะ ใช้ <b>spotlight</b>", n3: "หน้าที่ข้อมูลเยอะ ใช้ <b>zoom</b>" }
await call("storyboard", { repo, name: "mcp-smoke", scenes: lines.map(([clip, narration]) => ({ clip, narration, caption: captions[clip] })) })
await call("voice_preview", { text: "ทดสอบ สปาร์ก", repo })
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
await call("pr_text", { id: `${repo.split("/").pop()}--mcp-smoke` })
await call("purge", { older_than_days: 0 })
await call("delete", { id: `${repo.split("/").pop()}--mcp-smoke` })
await client.close()
console.log("\nALL PASSED")
