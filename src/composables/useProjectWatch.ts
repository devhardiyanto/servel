import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

export function useProjectWatch() {
  const watchedPath = ref<string | null>(null)
  const watching = ref(false)
  const error = ref<string | null>(null)

  async function watch(path: string): Promise<void> {
    watching.value = true
    error.value = null
    try {
      await invoke<void>('watch_project', { path })
      watchedPath.value = path
    } catch (err) {
      error.value = typeof err === 'string' ? err : String(err)
    } finally {
      watching.value = false
    }
  }

  async function pickFolder(): Promise<void> {
    const selected = await open({ directory: true, multiple: false })
    if (selected && typeof selected === 'string') {
      await watch(selected)
    }
  }

  return { watchedPath, watching, error, pickFolder, watch }
}
