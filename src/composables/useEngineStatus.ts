import { computed, ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { ENGINE_LABEL, type EngineKind, type PrereqStatus } from '@/types/prereq'

/** `missing`: engine terpilih tidak terpasang, bukan sekadar tidak menjawab. */
export type EngineState = 'up' | 'down' | 'missing' | 'unknown'

interface DaemonStatusPayload {
  running: boolean
  error: string | null
}

const engineStatus: Ref<EngineState> = ref('unknown')
const engine: Ref<EngineKind> = ref('docker')
/** WSLC tersedia tapi engine aktif Docker → UI menawarkan pindah. */
const wslcAvailable = ref(false)
const lastError: Ref<string | null> = ref(null)
let listenerRegistered = false

async function registerListener(): Promise<void> {
  if (listenerRegistered) return
  listenerRegistered = true
  try {
    // `docker-daemon-status` dipakai backend untuk engine apa pun (Docker maupun WSLC).
    await listen<DaemonStatusPayload>('docker-daemon-status', (e) => {
      // Polling tidak tahu beda "tidak terpasang" dan "mati"; jangan timpa `missing`.
      if (e.payload.running) engineStatus.value = 'up'
      else if (engineStatus.value !== 'missing') engineStatus.value = 'down'
      lastError.value = e.payload.error
    })
  } catch (err) {
    console.error('[useEngineStatus] listen failed', err)
    listenerRegistered = false
  }
}

async function refresh(): Promise<void> {
  try {
    const result = await invoke<PrereqStatus>('check_prerequisites')
    engine.value = result.engine
    wslcAvailable.value = result.wslc_available
    engineStatus.value = result.engine_running ? 'up' : result.engine_installed ? 'down' : 'missing'
    if (result.engine_running) {
      lastError.value = null
    }
  } catch (err) {
    console.error('[useEngineStatus] refresh failed', err)
  }
}

const engineLabel = computed(() => ENGINE_LABEL[engine.value])

/// Singleton — first caller registers the global event listener.
/// Use `refresh()` for explicit re-probe (e.g. retry button, app boot).
export function useEngineStatus() {
  void registerListener()
  return { engineStatus, engine, engineLabel, wslcAvailable, lastError, refresh }
}
