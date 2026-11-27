// Where walkie keeps its data on this Mac. The engine owns this folder; the Mac app reads it.
//   WALKIE_DATA   the whole folder
//   WALKIE_HOME   the video library only
//   WALKIE_QUEUE  the recording queue only
import { existsSync } from "node:fs"
import { homedir } from "node:os"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

export const DATA = process.env.WALKIE_DATA ?? join(homedir(), "Library", "Application Support", "walkie")
export const LIBRARY = process.env.WALKIE_HOME ?? join(DATA, "videos")
export const QUEUE = process.env.WALKIE_QUEUE ?? join(DATA, "queue")
export const PREVIEWS = join(DATA, "previews")

// The engine's own tools (profile, mix, frames, device, tts), one Rust binary that
// `walkie setup` builds into engine/bin/wt.
export const WT = join(dirname(fileURLToPath(import.meta.url)), "..", "bin", "wt")

export function checkWt() {
  if (!existsSync(WT)) throw new Error(`${WT} is missing: run ${join(dirname(WT), "walkie")} setup`)
}
