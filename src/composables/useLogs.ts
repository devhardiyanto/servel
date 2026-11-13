import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { pushBounded } from '@/utils/logBuffer'

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
    pushBounded(lines.value, { ts: timestamp(), src, text: payload.line, level })
  })
}

registerListener()

export function useLogs(defaultSrc = 'SERVEL') {
  globalDefaultSrc = defaultSrc


  function push(line: Omit<LogLine, 'src'> & { src?: string }): void {
    pushBounded(lines.value, { ...line, src: line.src ?? defaultSrc })
  }

  function clear(): void {
    lines.value = []
  }

  return { lines, push, clear }
}
