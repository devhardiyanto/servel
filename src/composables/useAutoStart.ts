import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useConfig } from './useConfig'
import { useServices, type StartOutcome } from './useServices'
import { useLogs } from './useLogs'
import { useEngineStatus } from './useEngineStatus'
import { ENGINE_LABEL, type PrereqStatus } from '@/types/prereq'

// Menyalakan service tersimpan saat Kova dibuka ("Start services" di Settings).
//
// v3.7.5:
// - Pilihan service dipulihkan di sini sebelum start. Dulu auto-start bergantung
//   pada `restoreServiceSelection` di App.vue yang berjalan bersamaan; bila
//   auto-start lebih dulu, pilihan masih kosong, `start()` tidak melakukan
//   apa-apa, tetapi log tetap menulis "auto-started".
// - Engine yang baru menyala (sesi WSL containers butuh beberapa detik) ditunggu
//   sampai `ENGINE_WAIT_MS`, bukan langsung dilewati.
// - Hasilnya disimpan di `state` dan ditampilkan oleh AutoStartNotice; log
//   mencatat hasil yang sebenarnya.

export type AutoStartPhase = 'idle' | 'waiting-engine' | 'starting' | 'done' | 'skipped' | 'failed'

export interface AutoStartState {
  phase: AutoStartPhase
  /** Kalimat untuk pengguna (bahasa Inggris, seperti UI lain). */
  message: string
  /** Nama service yang dimaksud. */
  services: string[]
  /** Alasan teknis bila gagal atau dilewati. */
  detail?: string
  /** Konflik port: arahkan ke Services, tempat banner konfliknya. */
  portConflict?: boolean
  /** Engine tidak siap: arahkan ke Settings → Container engine. */
  engineProblem?: boolean
}

const ENGINE_WAIT_MS = 60_000
const ENGINE_POLL_MS = 3_000

// Module-level: sekali per umur aplikasi (bukan per mount komponen).
let didAutoStart = false
const state = ref<AutoStartState>({ phase: 'idle', message: '', services: [] })

function nowTs(): string {
  return new Date().toTimeString().slice(0, 8)
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function probeEngine(): Promise<{ running: boolean; installed: boolean; label: string }> {
  try {
    const r = await invoke<PrereqStatus>('check_prerequisites')
    return { running: r.engine_running, installed: r.engine_installed, label: ENGINE_LABEL[r.engine] }
  } catch {
    return { running: false, installed: false, label: 'container engine' }
  }
}

async function run(): Promise<void> {
  const cfgApi = useConfig()
  const services = useServices()
  const { push } = useLogs('ENV')
  const log = (text: string) => push({ ts: nowTs(), src: 'ENV', text })

  await services.load()
  if (!cfgApi.loaded.value) await cfgApi.load()
  const cfg = cfgApi.config.value

  if (!cfg.autoStart) {
    log('auto-start: disabled')
    state.value = { phase: 'idle', message: '', services: [] }
    return
  }
  const ids = [...cfg.selectedServiceIds]
  if (ids.length === 0) {
    log('auto-start: no saved selection')
    state.value = { phase: 'idle', message: '', services: [] }
    return
  }

  const nameOf = (id: string) => services.definitions.value.find((d) => d.id === id)?.name ?? id
  const names = ids.map(nameOf)
  const list = names.join(', ')

  // Tunggu engine siap.
  let engine = await probeEngine()
  if (!engine.running && engine.installed) {
    state.value = { phase: 'waiting-engine', message: `Waiting for ${engine.label} to start ${list}…`, services: names }
    const deadline = Date.now() + ENGINE_WAIT_MS
    while (!engine.running && Date.now() < deadline) {
      await sleep(ENGINE_POLL_MS)
      engine = await probeEngine()
    }
    // Status bar ikut diperbarui tanpa menunggu putaran polling berikutnya.
    if (engine.running) void useEngineStatus().refresh()
  }
  if (!engine.running) {
    const why = engine.installed ? `${engine.label} is not responding` : `${engine.label} is not installed`
    log(`auto-start: skipped — ${why}`)
    state.value = { phase: 'skipped', message: `Saved services were not started: ${why}.`, services: names, detail: why, engineProblem: true }
    return
  }

  // Pulihkan pilihan sebelum start (lihat catatan di atas).
  services.setSelectedIds(ids)
  services.reconcileSelectedWithRunning()

  state.value = { phase: 'starting', message: `Starting ${list}…`, services: names }
  const outcome: StartOutcome = await services.start(ids)
  switch (outcome.kind) {
    case 'started':
      log(`auto-started: ${outcome.ids.join(', ')}`)
      state.value = { phase: 'done', message: `Started ${outcome.ids.map(nameOf).join(', ')}.`, services: names }
      break
    case 'already-running':
      log('auto-start: saved services are already running')
      state.value = { phase: 'done', message: `${list} ${names.length > 1 ? 'are' : 'is'} already running.`, services: names }
      break
    case 'nothing-selected':
      log('auto-start: skipped — no service selected')
      state.value = { phase: 'idle', message: '', services: [] }
      break
    case 'port-conflict': {
      const blocked = outcome.ids.map(nameOf).join(', ')
      log(`auto-start: skipped — port in use (${blocked})`)
      state.value = {
        phase: 'skipped',
        message: `${blocked} not started: its port is used by another program.`,
        services: names,
        portConflict: true,
      }
      break
    }
    case 'failed':
      log(`auto-start: failed — ${outcome.error}`)
      state.value = { phase: 'failed', message: `Could not start ${list}.`, services: names, detail: outcome.error }
      break
  }
}

export function useAutoStart() {
  async function runAutoStartOnce(): Promise<void> {
    if (didAutoStart) return
    didAutoStart = true
    await run()
  }

  /** Tombol Retry di AutoStartNotice. */
  async function retry(): Promise<void> {
    await run()
  }

  function dismiss(): void {
    state.value = { phase: 'idle', message: '', services: [] }
  }

  return { state, runAutoStartOnce, retry, dismiss }
}
