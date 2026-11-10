import { ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { PrereqStatus } from '@/types/prereq'

export type DockerDaemonState = 'up' | 'down' | 'unknown'

interface DaemonStatusPayload {
  running: boolean
  error: string | null
}

const dockerStatus: Ref<DockerDaemonState> = ref('unknown')
const lastError: Ref<string | null> = ref(null)
let listenerRegistered = false

async function registerListener(): Promise<void> {
  if (listenerRegistered) return
  listenerRegistered = true
  try {
    await listen<DaemonStatusPayload>('docker-daemon-status', (e) => {
      dockerStatus.value = e.payload.running ? 'up' : 'down'
      lastError.value = e.payload.error
    })
  } catch (err) {
    console.error('[useDockerStatus] listen failed', err)
    listenerRegistered = false
  }
}

async function refresh(): Promise<void> {
  try {
    const result = await invoke<PrereqStatus>('check_prerequisites')
    dockerStatus.value = result.docker_running ? 'up' : 'down'
    if (result.docker_running) {
      lastError.value = null
    }
  } catch (err) {
    console.error('[useDockerStatus] refresh failed', err)
  }
}

/// Singleton — first caller registers the global event listener.
/// Use `refresh()` for explicit re-probe (e.g. retry button, app boot).
export function useDockerStatus() {
  void registerListener()
  return { dockerStatus, lastError, refresh }
}
