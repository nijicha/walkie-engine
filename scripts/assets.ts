// Images a theme names (card logos, backgrounds), read into data URLs for the overlay. Shared by
// the recording (demo.ts) and the configurator preview (preview.ts).
import { readFileSync } from "node:fs"
import { extname } from "node:path"
import type { Profile } from "./overlay.ts"

const IMAGE_TYPES: Record<string, string> = { ".svg": "image/svg+xml", ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".webp": "image/webp" }

/** Card logos and background images of a theme, as path -> CSS url() with a data URL. */
export function cardAssets(theme: Profile["theme"]): Record<string, string> {
  const out: Record<string, string> = {}
  for (const st of Object.values(theme.cards ?? {})) {
    for (const path of [st?.logo, st?.background_image]) {
      if (!path || out[path]) continue
      const type = IMAGE_TYPES[extname(path).toLowerCase()] ?? "application/octet-stream"
      out[path] = `url(data:${type};base64,${readFileSync(path).toString("base64")})`
    }
  }
  return out
}
