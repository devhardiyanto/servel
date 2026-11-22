// Toast: konfirmasi singkat di kanan bawah. Hilang sendiri setelah 5
// detik kecuali punya aksi. Error tidak memakai toast; pakai banner.
import { ref } from 'vue'

export interface Toast {
  id: number
  text: string
  tone: 'ok' | 'info'
  action?: { label: string; run: () => void }
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

function show(text: string, opts: { tone?: Toast['tone']; action?: Toast['action'] } = {}): number {
  const id = nextId++
  toasts.value = [...toasts.value, { id, text, tone: opts.tone ?? 'ok', action: opts.action }].slice(-MAX)
  if (!opts.action) arm(id)
  return id
}

export function useToast() {
  return { toasts, show, dismiss, hold, release: (id: number) => { if (toasts.value.find((t) => t.id === id && !t.action)) arm(id) } }
}
