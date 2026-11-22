// Ringkasan RAM di status bar: total = service + project + Kova, diikuti
// rinciannya, supaya angka service sama persis dengan footer layar Services. Angka perkiraan (service yang belum melaporkan memori)
// diberi awalan "~", sama dengan layar Services.

export interface RamParts {
  servicesMb: number
  /** Ada service berjalan yang memakai perkiraan, bukan angka terukur. */
  servicesEstimated: boolean
  /** null = belum ada pengukuran proses. */
  projectsMb: number | null
  kovaMb: number | null
  /** Milidetik sejak epoch; null = belum diukur. */
  measuredAt: number | null
}

export function formatMb(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${Math.round(mb)} MB`
}

export function summarizeRam(r: RamParts): { text: string; title: string } | null {
  const total = r.servicesMb + (r.projectsMb ?? 0) + (r.kovaMb ?? 0)
  if (total <= 0) return null
  const est = r.servicesEstimated ? '~' : ''
  const parts = [`Services ${est}${formatMb(r.servicesMb)}`]
  if (r.projectsMb != null) parts.push(`Projects ${formatMb(r.projectsMb)}`)
  if (r.kovaMb != null) parts.push(`Kova ${formatMb(r.kovaMb)}`)
  // Rincian di teks: bagian yang nol tidak ditulis (mis. belum ada project berjalan).
  const shown: [string, number][] = []
  if (r.servicesMb > 0) shown.push(['services', r.servicesMb])
  if ((r.projectsMb ?? 0) > 0) shown.push(['projects', r.projectsMb ?? 0])
  if ((r.kovaMb ?? 0) > 0) shown.push(['Kova', r.kovaMb ?? 0])
  // Satu bagian saja: cukup namanya, angkanya sudah sama dengan total.
  const detail = shown.length === 1
    ? ` (${shown[0][0]})`
    : ` (${shown.map(([n, mb]) => `${n} ${n === 'services' ? est : ''}${formatMb(mb)}`).join(' + ')})`
  const text = `RAM ${est}${formatMb(total)}${detail}`
  const at = r.measuredAt ? ` — measured at ${new Date(r.measuredAt).toTimeString().slice(0, 8)}` : ''
  return { text, title: parts.join(' · ') + at }
}
