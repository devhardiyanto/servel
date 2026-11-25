// Toast: konfirmasi singkat di kanan bawah. Hilang sendiri setelah 5
// detik kecuali punya aksi. Toast `error` tidak pernah hilang sendiri,
// dibacakan sebagai alert, dan menawarkan Copy untuk pesannya.
import { ref } from 'vue'

export interface Toast {
  id: number
  text: string
  tone: 'ok' | 'info' | 'error'
  action?: { label: string; run: () => void }
  timed?: boolean
}

const toasts = ref<Toast[]>([])
const timers = new Map<number, ReturnType<typeof setTimeout>>()
let nextId = 1
const DURATION = 5000
const MAX = 3

function dismiss(id: number): void {
  clearTimeout(timers.get(id))
  timers.delete(id)
  toasts.value = toasts.value.filter((t) => t.id !== id)
}

function arm(id: number): void {
  clearTimeout(timers.get(id))
  timers.set(id, setTimeout(() => dismiss(id), DURATION))
}

/** Hover atau fokus menahan toast; lepas → hitung ulang. */
function hold(id: number): void {
  clearTimeout(timers.get(id))
  timers.delete(id)
}

/** `timed` lets a toast with an action still disappear by itself. */
function show(text: string, opts: { tone?: Toast['tone']; action?: Toast['action']; timed?: boolean } = {}): number {
  const id = nextId++
  const tone = opts.tone ?? 'ok'
  const timed = tone === 'error' ? false : opts.timed
  toasts.value = [...toasts.value, { id, text, tone, action: opts.action, timed }].slice(-MAX)
  if (tone !== 'error' && (!opts.action || opts.timed)) arm(id)
  return id
}

/** Error yang tetap terlihat sampai ditutup; Copy menyalin pesannya. */
function error(text: string, opts: { copy?: boolean } = {}): number {
  const action =
    opts.copy === false
      ? undefined
      : { label: 'Copy', run: () => void navigator.clipboard?.writeText(text).catch(() => undefined) }
  return show(text, { tone: 'error', action })
}

export function useToast() {
  return {
    toasts,
    show,
    error,
    dismiss,
    hold,
    release: (id: number) => {
      if (toasts.value.find((t) => t.id === id && t.tone !== 'error' && (!t.action || t.timed))) arm(id)
    },
  }
}
