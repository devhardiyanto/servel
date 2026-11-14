import { ref, onMounted } from 'vue'
// invoke langsung (bukan via call()) agar string error dari Rust bisa di-catch untuk node_switch dan node_install
import { invoke } from '@tauri-apps/api/core'
import { useTauri } from './useTauri'
import { useConfig } from './useConfig'
import { useVersionContext } from './useVersionContext'
import type { NodeVersion } from '@/types/version'

export function useNode() {
  const { call, on } = useTauri()
  const versionCtx = useVersionContext()

  const versions = ref<NodeVersion[]>([])
  const active = ref<string | null>(null)
  const switching = ref(false)
  const installing = ref(false)
  const error = ref<string | null>(null)


  async function load(): Promise<void> {
    const [versionList, activeVersion] = await Promise.all([
      call<NodeVersion[]>('node_list_installed'),
      call<string | null>('node_get_active'),
    ])
    if (versionList !== null) versions.value = versionList
    if (activeVersion !== undefined) active.value = activeVersion ?? null
  }

  // `scope` hanya dipakai provider mise: 'project' (pin di project aktif) atau
  // 'global'. Tanpa scope backend memilih otomatis, atau meminta keputusan
  // lewat MISE_NEEDS_SCOPE (ditangani useVersionContext → dialog di Dashboard).
  async function switchTo(version: string, scope?: 'project' | 'global'): Promise<void> {
    if (switching.value) return
    switching.value = true
    error.value = null
    try {
      await invoke<void>('node_switch', { version, scope: scope ?? null })
      await versionCtx.refresh()
      // Konfirmasi aktif versi dari Rust (optimistic + verify)
      const confirmed = await call<string | null>('node_get_active')
      if (confirmed !== undefined) {
        active.value = confirmed ?? null
        useConfig().setLastNodeVersion(active.value)
      }
    } catch (err) {
      if (!versionCtx.handleSwitchError('node', version, err)) {
        error.value = typeof err === 'string' ? err : String(err)
      }
    } finally {
      switching.value = false
    }
  }

  async function install(version: string): Promise<void> {
    installing.value = true
    error.value = null
    try {
      await invoke<void>('node_install', { version })
      await load()
    } catch (err) {
      error.value = typeof err === 'string' ? err : String(err)
    } finally {
      installing.value = false
    }
  }

  onMounted(async () => {
    await load()
    // Quick-switch dari tray menu → reuse switchTo (update active + persist).
    await on<{ kind: string; version: string }>('tray-switch-version', (payload) => {
      if (payload.kind === 'node') void switchTo(payload.version)
    })
  })

  return {
    versions,
    active,
    switching,
    installing,
    error,
    load,
    switchTo,
    install,
  }
}
