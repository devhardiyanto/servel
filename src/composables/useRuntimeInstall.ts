import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useToast } from '@/composables/useToast'

export type RuntimeTool = 'php' | 'node' | 'go' | 'python'

export interface RuntimeInstallJob {
  tool: RuntimeTool
  version: string
}

export interface RuntimeInstallResult extends RuntimeInstallJob {
  ok: boolean
  cancelled: boolean
  madeGlobal: boolean
  error: string
}

// Module scope: the Runtimes page unmounts when the user opens Logs, but the
// install keeps running in the backend and must still show when they return.
const current = ref<RuntimeInstallJob | null>(null)
const lastResult = ref<RuntimeInstallResult | null>(null)
/** `tool@version` installed during this app session, for the `new` chip. */
const fresh = ref(new Set<string>())

const LABELS: Record<RuntimeTool, string> = { php: 'PHP', node: 'Node', go: 'Go', python: 'Python' }

async function installedVersions(tool: RuntimeTool): Promise<string[]> {
  try {
    const o = await invoke<{ versions: { version: string; installed: boolean }[] }>('runtime_overview', { tool, projects: [] })
    return o.versions.filter((v) => v.installed).map((v) => v.version)
  } catch {
    return []
  }
}

/**
 * Runs `mise install`. The toast and the `new` chip are handled here, not in
 * the page, so they still happen when the install ends while Runtimes is closed.
 */
async function install(tool: RuntimeTool, version: string): Promise<RuntimeInstallResult | null> {
  if (current.value) return null
  const toast = useToast()
  const label = LABELS[tool]
  current.value = { tool, version }
  let result: RuntimeInstallResult
  let before = new Set<string>()
  try {
    before = new Set(await installedVersions(tool))
    const madeGlobal = await invoke<boolean>('runtime_install_version', { tool, version })
    result = { tool, version, ok: true, cancelled: false, madeGlobal: !!madeGlobal, error: '' }
  } catch (err) {
    const msg = String(err)
    result = { tool, version, ok: false, cancelled: msg.includes('cancelled'), madeGlobal: false, error: msg }
  } finally {
    current.value = null
  }
  if (result.ok) {
    toast.show(result.madeGlobal ? `Installed ${label} ${version} and set it as the global version` : `Installed ${label} ${version}`)
    markFresh((await installedVersions(tool)).filter((v) => !before.has(v)).map((v) => `${tool}@${v}`))
  } else if (result.cancelled) {
    toast.show(`Install of ${label} ${version} cancelled`)
  }
  lastResult.value = result
  return result
}

async function cancel(): Promise<void> {
  await invoke('runtime_install_cancel')
}

function markFresh(keys: string[]): void {
  if (keys.length === 0) return
  const next = new Set(fresh.value)
  for (const k of keys) next.add(k)
  fresh.value = next
}

export function useRuntimeInstall() {
  return { current, lastResult, fresh, install, cancel, markFresh }
}
