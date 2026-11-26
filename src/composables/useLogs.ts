import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { capAll, pushBounded } from '@/utils/logBuffer'

export interface LogLine {
  ts: string
  src: string
  text: string
  level?: 'warn' | 'error'
  /** Nomor urut naik terus; tetap benar walau baris lama dibuang buffer. */
  seq?: number
}

export interface ClearSnapshot {
  removed: LogLine[]
  gen: number
}

export interface CmdOutput {
  line: string
  stream: 'stdout' | 'stderr'
  source?: string
  level?: 'warn' | 'error'
}

const lines = ref<LogLine[]>([])
let nextSeq = 1
/** Clear terakhir; Undo dari toast yang lebih lama diabaikan. */
let clearGen = 0
let listenerRegistered = false
let globalDefaultSrc = 'KOVA'

function timestamp(): string {
  const d = new Date()
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

async function registerListener(): Promise<void> {
  if (listenerRegistered) return
  listenerRegistered = true
  await listen<CmdOutput>('cmd-output', (e) => {
    const payload = e.payload
    const src = payload.source ?? globalDefaultSrc
    const level = payload.level ?? (payload.stream === 'stderr' ? 'error' : undefined)
    pushBounded(lines.value, { ts: timestamp(), src, text: payload.line, level, seq: nextSeq++ })
  })
}

registerListener()

export function useLogs(defaultSrc = 'KOVA') {
  globalDefaultSrc = defaultSrc


  function push(line: Omit<LogLine, 'src'> & { src?: string }): void {
    pushBounded(lines.value, { ...line, src: line.src ?? defaultSrc, seq: nextSeq++ })
  }

  function clear(): void {
    lines.value = []
  }

  /** Hapus baris yang cocok; hasilnya dipakai `restore` untuk Undo. */
  function removeWhere(match: (l: LogLine) => boolean): ClearSnapshot {
    const removed = lines.value.filter(match)
    lines.value = lines.value.filter((l) => !match(l))
    return { removed, gen: ++clearGen }
  }

  /**
   * Kembalikan baris yang dihapus di posisi urutannya; baris yang masuk
   * sesudah Clear tetap ada. Batas buffer diterapkan lagi setelahnya.
   * `false` bila ada Clear yang lebih baru (Undo toast lama tidak berlaku).
   */
  function restore(snap: ClearSnapshot): boolean {
    if (snap.gen !== clearGen) return false
    const merged = [...lines.value, ...snap.removed].sort((a, b) => (a.seq ?? 0) - (b.seq ?? 0))
    lines.value = capAll(merged)
    return true
  }

  return { lines, push, clear, removeWhere, restore }
}
