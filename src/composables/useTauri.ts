import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { onUnmounted } from 'vue'

export function useTauri() {
  const unlisteners: UnlistenFn[] = []

  async function call<T>(command: string, args?: Record<string, unknown>): Promise<T | null> {
    try {
      return await invoke<T>(command, args)
    } catch (err) {
      console.error(`[tauri:${command}]`, err)
      return null
    }
  }

  async function on<T>(event: string, handler: (payload: T) => void): Promise<void> {
    const unlisten = await listen<T>(event, (e) => handler(e.payload))
    unlisteners.push(unlisten)
  }

  onUnmounted(() => {
    unlisteners.forEach((fn) => fn())
  })

  return { call, on }
}
