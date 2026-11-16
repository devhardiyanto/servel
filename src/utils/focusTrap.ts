// Fokus untuk overlay (modal, drawer): Tab berputar di dalam, dan fokus kembali
// ke elemen pemicu saat ditutup.
const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

export function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((el) => el.offsetParent !== null || el === document.activeElement)
}

/** Panggil di keydown Tab; mengembalikan true bila fokus dipindah. */
export function trapTab(root: HTMLElement, e: KeyboardEvent): boolean {
  if (e.key !== 'Tab') return false
  const items = focusables(root)
  if (items.length === 0) {
    e.preventDefault()
    return true
  }
  const first = items[0]
  const last = items[items.length - 1]
  const active = document.activeElement
  if (e.shiftKey && (active === first || !root.contains(active))) {
    e.preventDefault()
    last.focus()
    return true
  }
  if (!e.shiftKey && (active === last || !root.contains(active))) {
    e.preventDefault()
    first.focus()
    return true
  }
  return false
}
