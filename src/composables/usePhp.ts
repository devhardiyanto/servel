import { ref, onMounted } from 'vue'
// invoke langsung (bukan via call()) agar string error dari Rust bisa di-catch untuk php_switch dan php_install
import { invoke } from '@tauri-apps/api/core'
import { useTauri } from './useTauri'
import type { PhpVersion } from '@/types/version'

export function usePhp() {
  const { call } = useTauri()

  const versions = ref<PhpVersion[]>([])
  const active = ref<string | null>(null)
  const switching = ref(false)
  const installing = ref(false)
  const error = ref<string | null>(null)

  async function load(): Promise<void> {
    const [versionList, activeVersion] = await Promise.all([
      call<PhpVersion[]>('php_list_installed'),
      call<string | null>('php_get_active'),
    ])
    if (versionList !== null) versions.value = versionList
    if (activeVersion !== undefined) active.value = activeVersion ?? null
  }

  async function switchTo(version: string): Promise<void> {
    if (switching.value) return
    switching.value = true
    error.value = null
    try {
      await invoke<void>('php_switch', { version })
      // Konfirmasi aktif versi dari Rust (optimistic + verify)
      const confirmed = await call<string | null>('php_get_active')
      if (confirmed !== undefined) active.value = confirmed ?? null
    } catch (err) {
      error.value = typeof err === 'string' ? err : String(err)
    } finally {
      switching.value = false
    }
  }

  async function install(version: string): Promise<void> {
    installing.value = true
    error.value = null
    try {
      await invoke<void>('php_install', { version })
      await load()
    } catch (err) {
      error.value = typeof err === 'string' ? err : String(err)
    } finally {
      installing.value = false
    }
  }

  onMounted(load)

  return { versions, active, switching, installing, error, load, switchTo, install }
}
