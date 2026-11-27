// Semantic version order: 0.1.0-alpha < 0.1.0-beta < 0.1.0 < 0.1.1-alpha. Negative when a < b.
export function compareVersions(a: string, b: string) {
  const parse = (v: string) => {
    const [core, pre] = v.trim().replace(/^v/, "").split("-", 2)
    return { core: core.split(".").map(Number), pre: pre ? pre.split(".") : [] }
  }
  const x = parse(a), y = parse(b)
  for (let i = 0; i < 3; i++) if ((x.core[i] ?? 0) !== (y.core[i] ?? 0)) return (x.core[i] ?? 0) - (y.core[i] ?? 0)
  if (!x.pre.length || !y.pre.length) return y.pre.length - x.pre.length  // a release outranks its pre-releases
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const p = x.pre[i], q = y.pre[i]
    if (p === undefined || q === undefined) return p === undefined ? -1 : 1
    if (p === q) continue
    const n = /^\d+$/.test(p), m = /^\d+$/.test(q)
    if (n && m) return Number(p) - Number(q)
    if (n !== m) return n ? -1 : 1
    return p < q ? -1 : 1
  }
  return 0
}
