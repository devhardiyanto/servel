// Pintasan browser di jendela editor.
//
// Jendela utama memblokir pintasan browser di fase capture
// (`utils/webviewGuards.ts`). Di editor itu salah: CodeMirror melewati event
// yang sudah `defaultPrevented`, jadi Ctrl F, Ctrl D, Ctrl G, F12, Alt ← tidak
// akan jalan. Di sini guard dipasang di fase bubble dan hanya mematikan
// perilaku bawaan untuk tombol yang tidak dipakai editor. Di Windows,
// pintasan browser WebView2 juga dimatikan dari Rust (`editor::mod`).
import { shouldBlockKey } from '@/utils/webviewGuards'

export function installEditorGuards(): void {
  window.addEventListener('keydown', (e) => {
    if (!e.defaultPrevented && shouldBlockKey(e, true)) e.preventDefault()
  })
  window.addEventListener('wheel', (e) => { if (e.ctrlKey || e.metaKey) e.preventDefault() }, { passive: false })
  for (const type of ['dragover', 'drop'] as const) window.addEventListener(type, (e) => e.preventDefault())
  window.addEventListener('auxclick', (e) => e.preventDefault())
  window.addEventListener('mouseup', (e) => { if (e.button === 3 || e.button === 4) e.preventDefault() })
}

/** Pintasan editor dalam bentuk teks untuk dicocokkan ("Ctrl+Shift+P"). */
export function chord(e: KeyboardEvent): string {
  const parts: string[] = []
  if (e.ctrlKey || e.metaKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  let k = e.key
  if (k.length === 1) k = k.toUpperCase()
  if (k === '`' || k === '~') k = '`'
  parts.push(k)
  return parts.join('+')
}
