// Checks the schemas against every profile found and every video in the library, and checks that the
// schema defaults match profiles/default (the configurator shows the defaults from the schema).
// Run: node tests/schema/validate.ts
import { execFileSync } from "node:child_process"
import { existsSync, readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { Ajv2020 } from "ajv/dist/2020.js"
import { LIBRARY } from "../../scripts/paths.ts"

const ROOT = fileURLToPath(new URL("../..", import.meta.url))
const ajv = new Ajv2020({ allErrors: true, strict: false, validateFormats: false })
const read = (p: string) => JSON.parse(readFileSync(p, "utf8"))
const schemas = Object.fromEntries(["profile", "theme", "sequence", "voice", "manifest", "events"].map((n) =>
  [n, ajv.compile(read(join(ROOT, "schema", `${n}.schema.json`)))]))
let bad = 0
const check = (kind: string, file: string) => {
  const ok = schemas[kind](read(file))
  if (!ok) { bad++; console.log(`FAIL ${file}\n  ${ajv.errorsText(schemas[kind].errors, { separator: "\n  " })}`) }
  else console.log(`ok   ${file}`)
}

const profiles: Record<string, string> = JSON.parse(execFileSync(join(ROOT, "bin/wt"), ["profile", "--list"], { encoding: "utf8" }))
for (const dir of Object.values(profiles)) {
  for (const kind of ["profile", "theme", "sequence", "voice"]) {
    if (existsSync(join(dir, `${kind}.json`))) check(kind, join(dir, `${kind}.json`))
  }
}
// The English voice defaults (profiles/default/voice.en.json) are a voice file too.
check("voice", join(ROOT, "profiles/default/voice.en.json"))
const lib = LIBRARY
if (existsSync(lib)) for (const id of readdirSync(lib)) {
  if (existsSync(join(lib, id, "manifest.json"))) check("manifest", join(lib, id, "manifest.json"))
  if (existsSync(join(lib, id, "events.json"))) check("events", join(lib, id, "events.json"))
}
const smoke = join(process.env.TMPDIR ?? "/tmp", "walkie-smoke", "events.json")
if (existsSync(smoke)) check("events", smoke)

// Defaults in the schemas must equal profiles/default.
const defaults = (s: any): any => s.type === "object" && s.properties
  ? Object.fromEntries(Object.entries(s.properties).filter(([k]) => k !== "$schema").map(([k, v]: any) => [k, defaults(v)]).filter(([, v]) => v !== undefined))
  : s.default
for (const kind of ["theme", "sequence", "voice"]) {
  const fromSchema = defaults(read(join(ROOT, "schema", `${kind}.schema.json`)))
  const norm = (o: any): any => Array.isArray(o) ? o.map(norm) : o && typeof o === "object" ? Object.fromEntries(Object.keys(o).sort().map((k) => [k, norm(o[k])])) : o
  const a = JSON.stringify(norm(read(join(ROOT, "profiles/default", `${kind}.json`))))
  const b = JSON.stringify(norm(fromSchema))
  if (a !== b) { bad++; console.log(`FAIL defaults of ${kind}.schema.json differ from profiles/default/${kind}.json\n  default: ${a}\n  schema:  ${b}`) }
  else console.log(`ok   ${kind} defaults`)
}
process.exit(bad ? 1 : 0)
