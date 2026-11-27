import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Project, ProjectProcess } from '@/types/mise'

/** Mirror `SiteState` / `SiteRuntime` di src-tauri/src/sites/process.rs. */
export type SiteState = 'stopped' | 'starting' | 'running' | 'unresponsive' | 'exited'

export interface SiteRuntime {
  id: string
  state: SiteState
  exitCode: number | null
  lastLine: string | null
  /** Environment yang dipakai saat proses dimulai; null = Default. */
  envName?: string | null
  /** Port terakhir yang dicetak app di output (http://localhost:3000, listening on :8080). */
  printedPort?: number | null
}

/** Mirror `ProjectSuggestion` di src-tauri/src/sites/mod.rs. */
export interface ProjectSuggestion {
  name: string
  /** `reason`: alasan singkat deteksi, mis. "hono in package.json dependencies · src/index.ts". */
  detection: { kind: string; label: string; command: string; defaultPort: number; reason?: string }
  port: number
  /** Berkas yang membuat jenisnya dikenali. */
  evidence: string[]
}

/** Mirror `WorkerRuntime` di src-tauri/src/sites/workers.rs. */
export interface WorkerRuntime {
  projectId: string
  name: string
  state: SiteState
  exitCode: number | null
  lastLine: string | null
  /** Berapa kali Kova menjalankan ulang setelah crash. */
  restarts: number
  /** Kova berhenti mencoba setelah terlalu sering crash. */
  gaveUp: boolean
}

// Singleton: status dev server dibagi ke semua konsumen (Projects, Logs, popup tray, palet).
const runtime = ref<Record<string, SiteRuntime>>({})
const workers = ref<Record<string, WorkerRuntime>>({})
let listening = false

function apply(list: SiteRuntime[]): void {
  runtime.value = Object.fromEntries(list.map((r) => [r.id, r]))
}

function applyWorkers(list: WorkerRuntime[]): void {
  workers.value = Object.fromEntries(list.map((w) => [`${w.projectId}#${w.name}`, w]))
}

async function ensureListener(): Promise<void> {
  if (listening) return
  listening = true
  await listen<SiteRuntime[]>('site-status-changed', (e) => apply(e.payload))
  await listen<WorkerRuntime[]>('site-workers-changed', (e) => applyWorkers(e.payload))
  try {
    apply(await invoke<SiteRuntime[]>('sites_runtime'))
    applyWorkers(await invoke<WorkerRuntime[]>('workers_runtime'))
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

  function workerOf(projectId: string, name: string): WorkerRuntime | null {
    return workers.value[`${projectId}#${name}`] ?? null
  }

  async function startWorker(project: Project, name: string): Promise<void> {
    await invoke<void>('worker_start', { project, name })
  }

  async function stopWorker(projectId: string, name: string): Promise<void> {
    await invoke<void>('worker_stop', { projectId, name })
  }

  async function restartWorker(project: Project, name: string): Promise<void> {
    await invoke<void>('worker_restart', { project, name })
  }

  /** Usulan queue / scheduler / Vite untuk folder project. */
  async function processSuggestions(path: string, command: string): Promise<ProjectProcess[]> {
    return await invoke<ProjectProcess[]>('project_process_suggestions', { path, command })
  }

  return {
    runtime,
    workers,
    stateOf,
    infoOf,
    start,
    stop,
    restart,
    forget,
    detect,
    workerOf,
    startWorker,
    stopWorker,
    restartWorker,
    processSuggestions,
  }
}
