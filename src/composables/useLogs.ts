import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'

export interface LogLine {
  ts: string
  src: string
  text: string
  level?: 'warn' | 'error'
}

export interface CmdOutput {
  line: string
  stream: 'stdout' | 'stderr'
  source?: string
  level?: 'warn' | 'error'
}

const MAX_LINES = 400

// Singleton state — shared across all consumers
const lines = ref<LogLine[]>([])
let listenerRegistered = false
let globalDefaultSrc = 'SERVEL'

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
    lines.value.push({ ts: timestamp(), src, text: payload.line, level })
    if (lines.value.length > MAX_LINES) {
      lines.value = lines.value.slice(-MAX_LINES)
    }
  })
}

registerListener()

export function useLogs(defaultSrc = 'SERVEL') {
  globalDefaultSrc = defaultSrc


  function push(line: Omit<LogLine, 'src'> & { src?: string }): void {
    lines.value.push({ ...line, src: line.src ?? defaultSrc })
    if (lines.value.length > MAX_LINES) {
      lines.value = lines.value.slice(-MAX_LINES)
    }
  }

  function clear(): void {
    lines.value = []
  }

  return { lines, push, clear }
}
