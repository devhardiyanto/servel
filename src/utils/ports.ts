/** Port project di PC: bilangan bulat 1024–65535, selain itu `null`. */
export function projectPort(raw: string): number | null {
  const t = raw.trim()
  if (!/^\d{1,5}$/.test(t)) return null
  const n = Number(t)
  return n >= 1024 && n <= 65535 ? n : null
}

/** Port di dalam container: 1–65535; kosong = `0` (sama dengan port project). */
export function containerPort(raw: string): number | null {
  const t = raw.trim()
  if (t === '') return 0
  if (!/^\d{1,5}$/.test(t)) return null
  const n = Number(t)
  return n >= 1 && n <= 65535 ? n : null
}
