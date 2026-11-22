// Parser dan penulis teks .env untuk tab Environment.
//
// Aturan:
// - Baris kosong dan baris yang diawali `#` diabaikan.
// - Prefiks `export ` dibuang.
// - Pemisah adalah `=` PERTAMA: nilai boleh mengandung `=` (URL, query string).
// - Nilai dalam kutip "…" atau '…' dibuka kutipnya. Dalam kutip ganda, `\n`,
//   `\"` dan `\\` diterjemahkan. Tanpa kutip, ` #komentar` di ujung dibuang.
// - `PORT` dilewati: port diatur di Project settings dan diberikan Kova.
// - Nama dengan spasi atau `=` dilewati.

export interface EnvRow {
  k: string
  v: string
}

export interface SkippedLine {
  line: number
  text: string
  reason: string
}

export interface ParsedEnv {
  entries: EnvRow[]
  skipped: SkippedLine[]
}

const VALID_NAME = /^[^\s=]+$/

function unquote(raw: string): string {
  const v = raw.trim()
  if (v.length >= 2 && v.startsWith('"') && v.endsWith('"')) {
    return v
      .slice(1, -1)
      .replace(/\\(["\\n])/g, (_m, c: string) => (c === 'n' ? '\n' : c))
  }
  if (v.length >= 2 && v.startsWith("'") && v.endsWith("'")) return v.slice(1, -1)
  // Komentar di ujung hanya bila didahului spasi, supaya `a#b` tetap utuh.
  const hash = v.search(/\s#/)
  return (hash >= 0 ? v.slice(0, hash) : v).trim()
}

export function parseDotenv(text: string): ParsedEnv {
  const entries: EnvRow[] = []
  const skipped: SkippedLine[] = []
  const seen = new Map<string, number>()
  text.split(/\r?\n/).forEach((rawLine, i) => {
    const line = rawLine.trim()
    if (line === '' || line.startsWith('#')) return
    const body = line.replace(/^export\s+/, '')
    const eq = body.indexOf('=')
    if (eq <= 0) {
      skipped.push({ line: i + 1, text: line, reason: 'no KEY=VALUE' })
      return
    }
    const k = body.slice(0, eq).trim()
    if (!VALID_NAME.test(k)) {
      skipped.push({ line: i + 1, text: line, reason: `"${k}" is not a valid name` })
      return
    }
    if (k.toUpperCase() === 'PORT') {
      skipped.push({ line: i + 1, text: line, reason: 'PORT is set in Project settings' })
      return
    }
    const v = unquote(body.slice(eq + 1))
    // Nama ganda dalam satu teks: yang terakhir menang (seperti dotenv).
    const at = seen.get(k)
    if (at !== undefined) entries[at] = { k, v }
    else {
      seen.set(k, entries.length)
      entries.push({ k, v })
    }
  })
  return { entries, skipped }
}

function quote(v: string): string {
  // Path Windows (`C:\\Program Files\\…`) tetap apa adanya bila tanpa spasi.
  if (v === '' || /^[^\s#"']+$/.test(v)) return v
  // Kutip tunggal = literal, tanpa escape; dipakai bila memungkinkan.
  if (!v.includes("'") && !v.includes('\n')) return `'${v}'`
  return `"${v.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n')}"`
}

/** Baris → teks .env yang bisa dibaca ulang `parseDotenv` tanpa berubah. */
export function serializeDotenv(rows: EnvRow[]): string {
  return rows
    .filter((r) => r.k.trim() !== '' || r.v !== '')
    .map((r) => `${r.k.trim()}=${quote(r.v)}`)
    .join('\n')
}

export interface MergeResult {
  rows: EnvRow[]
  added: number
  replaced: number
}

/**
 * Gabungkan hasil paste ke baris yang ada: nama yang sudah ada diganti
 * nilainya, nama baru ditambahkan di posisi `at` (baris paste). Baris kosong
 * di posisi itu dibuang.
 */
export function mergeRows(rows: EnvRow[], incoming: EnvRow[], at = rows.length): MergeResult {
  const next = rows.map((r) => ({ ...r }))
  let added = 0
  let replaced = 0
  const fresh: EnvRow[] = []
  for (const e of incoming) {
    const i = next.findIndex((r) => r.k.trim() === e.k)
    if (i >= 0) {
      if (next[i].v !== e.v) replaced++
      next[i].v = e.v
    } else {
      fresh.push({ ...e })
      added++
    }
  }
  const pos = Math.min(Math.max(at, 0), next.length)
  const blankAtPos = next[pos] && next[pos].k.trim() === '' && next[pos].v === ''
  next.splice(pos, blankAtPos ? 1 : 0, ...fresh)
  return { rows: next, added, replaced }
}

/** "4 added · 1 replaced · 1 skipped" untuk ringkasan paste. */
export function summarize(added: number, replaced: number, skipped: number): string {
  return `${added} added · ${replaced} replaced · ${skipped} skipped`
}
