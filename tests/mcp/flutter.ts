// End-to-end check of the Flutter path of the MCP server: node tests/mcp/flutter.ts [device]
// Makes a throwaway Flutter app (flutter create + tests/flutter/main.dart) in a temp git repo, then
// start -> storyboard -> record on a simulator or emulator -> texts -> rerender -> delete.
// LANDSCAPE=1 turns the app to landscape (the full layout). KEEP=1 keeps the video in the library.
// Needs Flutter (WALKIE_FLUTTER, default "flutter", e.g. "puro flutter") and a simulator or
// emulator (device: name, UDID or serial; default the running one).
import { Client } from "@modelcontextprotocol/client"
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio"
import { execFileSync } from "node:child_process"
import { cpSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { basename, dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const SKILL = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const device = process.argv[2]
const flutter = (process.env.WALKIE_FLUTTER ?? "flutter").split(" ")
const sh = (cmd: string[], cwd: string) => execFileSync(cmd[0], cmd.slice(1), { cwd, stdio: ["ignore", "ignore", "inherit"] })

const repo = join(mkdtempSync(join(tmpdir(), "wt-flutter-")), "wt_fixture")
console.log(`app: ${repo}`)
sh([...flutter, "create", "--platforms", "ios,android", "--project-name", "wt_fixture", repo], dirname(repo))
cpSync(join(SKILL, "tests/flutter/main.dart"), join(repo, "lib/main.dart"))
// The app asks for contacts on start (native code): record must grant it so no prompt shows.
cpSync(join(SKILL, "tests/flutter/ios/AppDelegate.swift"), join(repo, "ios/Runner/AppDelegate.swift"))
const kotlin = join(repo, "android/app/src/main/kotlin/com/example/wt_fixture/MainActivity.kt")
cpSync(join(SKILL, "tests/flutter/android/MainActivity.kt"), kotlin)
const plist = join(repo, "ios/Runner/Info.plist")
writeFileSync(plist, readFileSync(plist, "utf8").replace("<dict>",
  "<dict>\n\t<key>NSContactsUsageDescription</key>\n\t<string>Shows who a wristband belongs to.</string>"))
const manifest = join(repo, "android/app/src/main/AndroidManifest.xml")
writeFileSync(manifest, readFileSync(manifest, "utf8").replace("<application",
  '<uses-permission android:name="android.permission.READ_CONTACTS"/>\n    <application'))
sh([...flutter, "pub", "add", "dev:integration_test:{\"sdk\":\"flutter\"}"], repo)
writeFileSync(join(repo, ".gitignore"), readFileSync(join(repo, ".gitignore"), "utf8") + "\ntmp/\n")
sh(["git", "init", "-q"], repo)

const env = {
  ...process.env, WALKIE_HOME: process.env.WALKIE_HOME ?? "/tmp/walkie-test/videos",
  DEMO_PROFILE: "default",
} as Record<string, string>
const client = new Client({ name: "walkie-flutter-test", version: "0" })
await client.connect(new StdioClientTransport({ command: join(SKILL, "bin/walkie"), args: ["mcp"], env }))
const call = async (name: string, args: Record<string, unknown> = {}) => {
  const r: any = await client.callTool({ name, arguments: args }, { timeout: 20 * 60_000 }) // record takes minutes
  const out = r.content.map((c: any) => c.text).join("\n")
  console.log(`\n=== ${name}${r.isError ? " (ERROR)" : ""}\n${out}`)
  if (r.isError) {
    console.error(`FAIL: ${name}`)
    process.exit(1)
  }
  return out
}

const name = "flutter-smoke"
const started = await call("start", { repo, name })
if (!started.includes("Runner: flutter")) throw new Error("start did not pick the flutter runner")
const dir = started.match(/Walkie folder: (.+)/)![1]
// LANDSCAPE=1: the app turns to landscape before the intro (the full layout, a rotated recording).
let spec = readFileSync(join(SKILL, "tests/flutter/demo_test.dart"), "utf8")
if (process.env.LANDSCAPE) {
  spec = spec
    .replace("import 'package:flutter_test/flutter_test.dart';", "import 'package:flutter/services.dart';\nimport 'package:flutter_test/flutter_test.dart';")
    .replace("    app.main();", "    await SystemChrome.setPreferredOrientations([DeviceOrientation.landscapeLeft]);\n    app.main();")
}
writeFileSync(join(dir, "demo_test.dart"), spec)
// The outro is off in the default profile; turn it on here to exercise demo.outro().
const prof = JSON.parse(readFileSync(join(dir, "profile.json"), "utf8"))
prof.sequence = { ...prof.sequence, outro: true, outro_html: "<h1>ขอบคุณ</h1>" }
writeFileSync(join(dir, "profile.json"), JSON.stringify(prof, null, 1))
await call("storyboard", { repo, name, scenes: [
  { clip: "n0", narration: "วิธีตั้งค่าการแจ้งเตือน ทางอีเมลบนแอปมือถือ" },
  { clip: "n1", caption: "กรอกอีเมล แล้วกด <b>“Save”</b>", narration: "แตะช่องอีเมล กรอกอีเมลแล้วกดเซฟ" },
  { clip: "n2", caption: "เลื่อนลงไปดู <b>Wristband 12</b>", narration: "เลื่อนลงไปดูรายการ ริสต์แบนด์ที่ลงทะเบียนไว้" },
  { clip: "n3", caption: "เปิด <b>Email alerts</b>", narration: "ไปที่หน้าตั้งค่า แล้วเปิดการแจ้งเตือนทางอีเมล" },
  { clip: "n9", narration: "สรุป กรอกอีเมลแล้วกดเซฟ ก็รับการแจ้งเตือนได้เลย" },
] })
await call("record", { repo, name, title: "Flutter smoke test", ...(device ? { device } : {}) })
const id = `${basename(repo)}--${name}`
await call("texts", { id })
await call("rerender", { id, text: [{ id: "step-1", html: '<span class="step">1</span>แก้ข้อความ <b>หลังอัด</b>' }] })
if (!process.env.KEEP) await call("delete", { id })
await client.close()
