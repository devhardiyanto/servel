import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useToast } from '@/composables/useToast'

/** php, node, go, python, atau tool tambahan dari Add tool. */
export type RuntimeTool = string

export interface RuntimeInstallJob {
  tool: RuntimeTool
  version: string
  /** Distro WSL; kosong = PC ini. */
  distro?: string | null
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

const LABELS: Record<string, string> = { php: 'PHP', node: 'Node', go: 'Go', python: 'Python' }
export function toolLabel(tool: string): string {
  return LABELS[tool] ?? tool.charAt(0).toUpperCase() + tool.slice(1)
}
/** Kunci chip `new`: `tool@versi`, diawali `distro:` untuk versi di WSL. */
export function freshKey(tool: string, version: string, distro?: string | null): string {
  return `${distro ? `${distro}:` : ''}${tool}@${version}`
}

async function installedVersions(tool: RuntimeTool, distro?: string | null): Promise<string[]> {
  try {
    const o = await invoke<{ versions: { version: string; installed: boolean }[] }>('runtime_overview', { tool, projects: [], distro: distro || null })
    return o.versions.filter((v) => v.installed).map((v) => v.version)
  } catch {
    return []
  }
}

/**
 * Runs `mise install`. The toast and the `new` chip are handled here, not in
 * the page, so they still happen when the install ends while Runtimes is closed.
 */
async function install(tool: RuntimeTool, version: string, distro?: string | null): Promise<RuntimeInstallResult | null> {
  if (current.value) return null
  const toast = useToast()
  const label = toolLabel(tool)
  const where = distro ? ` in ${distro}` : ''
  current.value = { tool, version, distro: distro || null }
  let result: RuntimeInstallResult
  let before = new Set<string>()
  try {
    before = new Set(await installedVersions(tool, distro))
    const madeGlobal = await invoke<boolean>('runtime_install_version', { tool, version, distro: distro || null })
    result = { tool, version, distro: distro || null, ok: true, cancelled: false, madeGlobal: !!madeGlobal, error: '' }
  } catch (err) {
    const msg = String(err)
    result = { tool, version, distro: distro || null, ok: false, cancelled: msg.includes('cancelled'), madeGlobal: false, error: msg }
  } finally {
    current.value = null
  }
  if (result.ok) {
    toast.show(result.madeGlobal ? `Installed ${label} ${version}${where} and set it as the global version` : `Installed ${label} ${version}${where}`)
    markFresh((await installedVersions(tool, distro)).filter((v) => !before.has(v)).map((v) => freshKey(tool, v, distro)))
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
