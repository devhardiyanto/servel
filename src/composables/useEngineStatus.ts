import { computed, ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { ENGINE_LABEL, type EngineKind, type PrereqStatus } from '@/types/prereq'

export type EngineState = 'up' | 'down' | 'unknown'

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
    // Nama event dipertahankan dari v1.5 (backend emit untuk engine apa pun).
    await listen<DaemonStatusPayload>('docker-daemon-status', (e) => {
      engineStatus.value = e.payload.running ? 'up' : 'down'
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
    engineStatus.value = result.engine_running ? 'up' : 'down'
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
