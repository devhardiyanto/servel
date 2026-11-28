// Pencocokan Quick Open: huruf query harus muncul berurutan di path.
// Skor lebih tinggi untuk huruf yang cocok di nama file, di awal kata, dan
// berurutan; path pendek menang bila skornya sama.

export interface Match { path: string; score: number; hits: number[] }

const SEP = /[\\/_\-. ]/

export function fuzzy(query: string, path: string): Match | null {
  const q = query.toLowerCase().replace(/\s+/g, '')
  if (!q) return { path, score: 0, hits: [] }
  const p = path.toLowerCase()
  const nameStart = path.lastIndexOf('/') + 1
  const hits: number[] = []
  let score = 0
  let qi = 0
  let prev = -2
  for (let i = 0; i < p.length && qi < q.length; i++) {
    if (p[i] !== q[qi]) continue
    let s = 1
    if (i >= nameStart) s += 2
    if (i === 0 || SEP.test(path[i - 1]) || (path[i] >= 'A' && path[i] <= 'Z' && path[i - 1] >= 'a' && path[i - 1] <= 'z')) s += 3
    if (prev === i - 1) s += 2
    score += s
    hits.push(i)
    prev = i
    qi++
  }
  if (qi < q.length) return null
  return { path, score: score - path.length * 0.01, hits }
}

export function rank(query: string, paths: string[], limit = 40): Match[] {
  const out: Match[] = []
  for (const p of paths) {
    const m = fuzzy(query, p)
    if (m) out.push(m)
  }
  out.sort((a, b) => b.score - a.score)
  return out.slice(0, limit)
}
