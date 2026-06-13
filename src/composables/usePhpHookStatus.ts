import { ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export type PhpHookStatus = 'installed' | 'not_installed' | 'unknown'

const status: Ref<PhpHookStatus> = ref('unknown')
let initialized = false

async function refresh(): Promise<void> {
  try {
    const result = await invoke<string>('php_hook_status')
    if (result === 'installed' || result === 'not_installed' || result === 'unknown') {
      status.value = result
    } else {
      status.value = 'unknown'
    }
  } catch {
    status.value = 'unknown'
  }
}

/// Singleton — first caller triggers an initial probe; subsequent callers
/// reuse the shared ref and can manually `refresh()` if needed.
export function usePhpHookStatus() {
  if (!initialized) {
    initialized = true
    void refresh()
  }
  return { status, refresh }
}
