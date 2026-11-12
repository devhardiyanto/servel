import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Project } from '@/types/mise'

/** Mirror `SiteState` / `SiteRuntime` di src-tauri/src/sites/process.rs. */
export type SiteState = 'stopped' | 'starting' | 'running' | 'unresponsive' | 'exited'

export interface SiteRuntime {
  id: string
  state: SiteState
  exitCode: number | null
  lastLine: string | null
}

/** Mirror `ProjectSuggestion` di src-tauri/src/sites/mod.rs. */
export interface ProjectSuggestion {
  name: string
  detection: { kind: string; label: string; command: string; defaultPort: number }
  port: number
}

export interface LegacyStatus {
  hostsBlock: boolean
  cert: boolean
}

// Singleton: status dev server dibagi ke semua konsumen (Settings, Dashboard).
const runtime = ref<Record<string, SiteRuntime>>({})
let listening = false

function apply(list: SiteRuntime[]): void {
  runtime.value = Object.fromEntries(list.map((r) => [r.id, r]))
}

async function ensureListener(): Promise<void> {
  if (listening) return
  listening = true
  await listen<SiteRuntime[]>('site-status-changed', (e) => apply(e.payload))
  try {
    apply(await invoke<SiteRuntime[]>('sites_runtime'))
  } catch (err) {
    console.error('[sites_runtime]', err)
  }
}

export function isActive(state: SiteState): boolean {
  return state === 'starting' || state === 'running' || state === 'unresponsive'
}

export function useProjectRuntime() {
  void ensureListener()

  function stateOf(id: string): SiteState {
    return runtime.value[id]?.state ?? 'stopped'
  }

  function infoOf(id: string): SiteRuntime | null {
    return runtime.value[id] ?? null
  }

  async function start(project: Project): Promise<void> {
    await invoke<void>('site_start', { project })
  }

  /** Stop lalu start; menunggu port bebas di antaranya. */
  async function restart(project: Project): Promise<void> {
    await invoke<void>('site_restart', { project })
  }

  async function stop(id: string): Promise<void> {
    await invoke<void>('site_stop', { id })
  }

  async function forget(id: string): Promise<void> {
    await invoke<void>('site_forget', { id })
  }

  /** Kenali project di `path`; `taken` = port project lain. */
  async function detect(path: string, taken: number[]): Promise<ProjectSuggestion> {
    return await invoke<ProjectSuggestion>('project_detect', { path, taken })
  }

  return { runtime, stateOf, infoOf, start, stop, restart, forget, detect }
}
