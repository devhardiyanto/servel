import { ref, onMounted } from 'vue'
import { useTauri } from './useTauri'

export interface LogLine {
  ts: string
  src: string
  text: string
  level?: 'warn' | 'error'
}

interface CmdOutput {
  line: string
  stream: 'stdout' | 'stderr'
}

const MAX_LINES = 400

function timestamp(): string {
  const d = new Date()
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

export function useLogs(defaultSrc = 'SERVEL') {
  const { on } = useTauri()
  const lines = ref<LogLine[]>([])

  function push(line: LogLine): void {
    lines.value.push(line)
    if (lines.value.length > MAX_LINES) {
      lines.value = lines.value.slice(-MAX_LINES)
    }
  }

  function clear(): void {
    lines.value = []
  }

  onMounted(async () => {
    await on<CmdOutput>('cmd-output', (payload) => {
      push({
        ts: timestamp(),
        src: defaultSrc,
        text: payload.line,
        level: payload.stream === 'stderr' ? 'error' : undefined,
      })
    })
  })

  return { lines, push, clear }
}
