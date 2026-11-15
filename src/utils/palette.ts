// Pencarian palet perintah (Ctrl K): setiap kata harus muncul di label atau
// kata kunci; urutan hasil mengutamakan awalan label, lalu awalan kata.

export interface PaletteCommand {
  id: string
  label: string
  /** Kelompok yang ditampilkan di kanan baris. */
  group: string
  /** Teks tambahan untuk pencarian (tidak ditampilkan). */
  keywords?: string
  run: () => void | Promise<void>
}

function score(cmd: PaletteCommand, words: string[]): number {
  const label = cmd.label.toLowerCase()
  const hay = `${label} ${cmd.group.toLowerCase()} ${(cmd.keywords ?? '').toLowerCase()}`
  let total = 0
  for (const w of words) {
    if (!hay.includes(w)) return -1
    if (label.startsWith(w)) total += 3
    else if (label.split(/[\s:]+/).some((part) => part.startsWith(w))) total += 2
    else if (label.includes(w)) total += 1
  }
  return total
}

/** Kosong = semua perintah dalam urutan asli. Urutan stabil untuk skor yang sama. */
export function filterCommands<T extends PaletteCommand>(commands: T[], query: string): T[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return commands
  return commands
    .map((cmd, i) => ({ cmd, i, s: score(cmd, words) }))
    .filter((x) => x.s >= 0)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((x) => x.cmd)
}
