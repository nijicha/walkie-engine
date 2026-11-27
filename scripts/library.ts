// The video library on this Mac: one folder per walkie (<repo>--<name>) with manifest.json and
// the layers. Shared by the MCP server and the Mac app, so both read it the same way.
import { existsSync, readdirSync, readFileSync } from "node:fs"
import { basename, join } from "node:path"
import { LIBRARY } from "./paths.ts"

export type Usage = {
  sessions: string[]; from: string; to: string; calls: number; models: string[]
  input: number; cache_write: number; cache_read: number; output: number
}

export type Manifest = {
  schema_version: number
  id: string; name: string; title: string | null; pr: string | null
  repo: string; repo_path: string; branch: string; commit: string; profile: string
  created_at: string; revoiced_at?: string; rerendered_at?: string
  duration_s: number; flashes: number
  clips: { name: string; start: number }[]
  usage?: Usage | null
  files: string[]
}

export type Scene = { clip: string; caption?: string; highlight?: string; action?: string; narration: string }
export type TextItem = { id: string; t: number; html: string }

const readJson = (p: string) => JSON.parse(readFileSync(p, "utf8"))

export const videoDir = (id: string) => join(LIBRARY, id)

/** A library id is <repo folder>--<name>; anything else is refused before touching the disk. */
export const isVideoId = (id: string) => /^[\w.-]+--[a-z0-9-]+$/.test(id)

/** Every video in the library, newest first. */
export function listLibrary(): Manifest[] {
  if (!existsSync(LIBRARY)) return []
  return readdirSync(LIBRARY)
    .map((id) => join(LIBRARY, id, "manifest.json"))
    .filter(existsSync)
    .map(readJson)
    .sort((a, b) => (a.created_at < b.created_at ? 1 : -1))
}

/** The projects (git repos) that have videos, newest activity first. */
export function listProjects() {
  const byRepo = new Map<string, { path: string; name: string; videos: number; last: string }>()
  for (const m of listLibrary()) {
    const p = byRepo.get(m.repo_path) ?? { path: m.repo_path, name: basename(m.repo_path), videos: 0, last: m.created_at }
    p.videos += 1
    if (m.created_at > p.last) p.last = m.created_at
    byRepo.set(m.repo_path, p)
  }
  return [...byRepo.values()].sort((a, b) => (a.last < b.last ? 1 : -1))
}

/** The text layer: captions and cards in events.json, by id. Empty for a video without layers. */
export function textItems(id: string): TextItem[] {
  const file = join(videoDir(id), "events.json")
  if (!existsSync(file)) return []
  return readJson(file).events
    .filter((e: any) => e.id && (e.type === "caption" || e.type === "card"))
    .map((e: any) => ({ id: e.id, t: e.t, html: e.html }))
}

/** Narration lines by clip, from narration.tsv. */
export function narration(id: string): Record<string, string> {
  const file = join(videoDir(id), "narration.tsv")
  if (!existsSync(file)) return {}
  return Object.fromEntries(readFileSync(file, "utf8").split("\n").filter(Boolean).map((l) => {
    const [clip, ...rest] = l.split("\t")
    return [clip, rest.join("\t")]
  }))
}

export function storyboard(id: string): Scene[] {
  const file = join(videoDir(id), "storyboard.json")
  return existsSync(file) ? readJson(file) : []
}

/** One video with what a viewer needs: manifest, scenes, narration and text by clip. */
export function readVideo(id: string) {
  if (!isVideoId(id) || !existsSync(join(videoDir(id), "manifest.json"))) return null
  const manifest: Manifest = readJson(join(videoDir(id), "manifest.json"))
  return { manifest, dir: videoDir(id), scenes: storyboard(id), narration: narration(id), texts: textItems(id) }
}

/** Markdown for a PR description: what the video shows, scene by scene, with a marked spot where the
 *  video goes once someone drags the MP4 into GitHub (GitHub has no API for video attachments). */
export function prText(m: Manifest, scenes: Scene[]) {
  const plain = (s = "") => s.replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim()
  const mmss = (s: number) => `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, "0")}`
  const rows = scenes
    .filter((s) => s.caption || s.action)
    .map((s, i) => `| ${i + 1} | ${plain(s.caption) || plain(s.action)} | ${plain(s.narration)} |`)
  return [
    `### Walkie${m.title ? `: ${m.title}` : ""}`,
    "",
    "<!-- Drag walkie.mp4 here -->",
    "",
    `Length ${mmss(m.duration_s)}, recorded on \`${m.branch}\` at \`${m.commit}\`.`,
    ...(rows.length ? ["", "| Step | On screen | Narration |", "|---|---|---|", ...rows] : []),
  ].join("\n")
}
