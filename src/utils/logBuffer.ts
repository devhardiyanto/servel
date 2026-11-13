/**
 * Batas buffer log. Tiap sumber (service atau project) punya jatah sendiri,
 * supaya dev server yang ramai tidak mendorong keluar baris error project
 * lain. `MAX_TOTAL` hanya pengaman memori.
 */
export const PER_SOURCE = 500
export const MAX_TOTAL = 4000

/** Tambah `line` ke `lines` (di tempat), lalu buang yang terlama bila lewat batas. */
export function pushBounded<T extends { src: string }>(
  lines: T[],
  line: T,
  perSource = PER_SOURCE,
  maxTotal = MAX_TOTAL,
): void {
  lines.push(line)
  let count = 0
  for (const l of lines) if (l.src === line.src) count++
  if (count > perSource) {
    lines.splice(
      lines.findIndex((l) => l.src === line.src),
      1,
    )
  }
  if (lines.length > maxTotal) lines.splice(0, lines.length - maxTotal)
}
