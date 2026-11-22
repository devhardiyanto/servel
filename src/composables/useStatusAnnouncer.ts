// Umumkan perubahan status project ke pembaca layar: perubahan biasa lewat region
// role="status" (polite), project yang berhenti dengan error lewat
// role="alert". Status awal saat aplikasi dibuka tidak diumumkan.
import { ref, watch } from 'vue'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime, type SiteRuntime, type SiteState } from '@/composables/useProjectRuntime'

const polite = ref('')
const assertive = ref('')

/** Kalimat untuk satu perubahan; `null` = tidak perlu diumumkan. */
export function describeChange(name: string, prev: SiteState, next: SiteRuntime): string | null {
  if (prev === next.state) return null
  switch (next.state) {
    case 'starting':
      return `${name} is starting`
    case 'running':
      return `${name} is running`
    case 'unresponsive':
      return `${name} is running but not answering on its port`
    case 'stopped':
      return prev === 'starting' || prev === 'running' || prev === 'unresponsive' ? `${name} stopped` : null
    case 'exited':
      return next.exitCode !== null ? `${name} stopped with exit code ${next.exitCode}` : `${name} stopped`
  }
}

// Ulangi teks yang sama tetap terbaca: kosongkan dulu, isi di tick berikutnya.
function say(target: typeof polite, text: string): void {
  target.value = ''
  setTimeout(() => (target.value = text), 50)
}

export function useStatusAnnouncer() {
  const { config } = useConfig()
  const { runtime } = useProjectRuntime()
  let known: Record<string, SiteState> | null = null

  watch(
    runtime,
    (next) => {
      const states = Object.fromEntries(Object.values(next).map((r) => [r.id, r.state]))
      if (known === null) {
        known = states
        return
      }
      for (const r of Object.values(next)) {
        const name = config.value.projects.find((p) => p.id === r.id)?.name
        if (!name) continue
        const text = describeChange(name, known[r.id] ?? 'stopped', r)
        if (!text) continue
        if (r.state === 'exited' && r.exitCode !== null && r.exitCode !== 0) say(assertive, text)
        else say(polite, text)
      }
      known = states
    },
    { deep: true },
  )

  return { polite, assertive }
}
