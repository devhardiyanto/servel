// Ringkasan RAM di status bar: total = service + project + Kova, Kova juga
// ditulis terpisah. Angka perkiraan (service yang belum melaporkan memori)
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
  const text = `RAM ${est}${formatMb(total)}${r.kovaMb != null ? ` · Kova ${formatMb(r.kovaMb)}` : ''}`
  const parts = [`Services ${est}${formatMb(r.servicesMb)}`]
  if (r.projectsMb != null) parts.push(`Projects ${formatMb(r.projectsMb)}`)
  if (r.kovaMb != null) parts.push(`Kova ${formatMb(r.kovaMb)}`)
  const at = r.measuredAt ? ` — measured at ${new Date(r.measuredAt).toTimeString().slice(0, 8)}` : ''
  return { text, title: parts.join(' · ') + at }
}
