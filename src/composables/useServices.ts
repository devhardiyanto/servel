import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useTauri } from './useTauri'
import { useConfig } from './useConfig'
import { useLogs } from './useLogs'
import type { ServiceDef, ServiceStatus, ServiceUiState, ContainerMemStat, PortConflict } from '@/types/service'

interface ContainerStatusChangedPayload {
  service: string
  running: boolean
}

/**
 * Hasil `start()`. `start()` bisa batal tanpa menjalankan apa pun (pilihan
 * kosong, konflik port); pemanggil seperti auto-start memeriksa hasil ini
 * sebelum mencatat "started".
 */
export type StartOutcome =
  | { kind: 'started'; ids: string[] }
  | { kind: 'already-running' }
  | { kind: 'nothing-selected' }
  | { kind: 'port-conflict'; ids: string[] }
  | { kind: 'failed'; error: string }

interface ServicesActionPayload {
  /** `stop`: satu service dihentikan dari popup tray. */
  action: 'start' | 'stop' | 'stop_all'
  services: string[]
}

// Singleton state — shared across all consumers
const definitions = ref<ServiceDef[]>([])
const statuses = ref<ServiceStatus[]>([])
const uiState = ref<Record<string, ServiceUiState>>({})
const serviceError = ref<string | null>(null)
const portConflicts = ref<PortConflict[]>([])
let initialized = false
let listenerRegistered = false
let servicesActionListenerRegistered = false
let statsListenerRegistered = false
let persistWatchRegistered = false

function deriveStatus(id: string): ServiceUiState['status'] {
  const entry = statuses.value.find((s) => s.id === id)
  if (!entry) return 'not_created'
  if (entry.running) return 'running'
  if (entry.state === 'exited' || entry.state === 'dead') return 'stopped'
  if (entry.state === 'paused') return 'stopped'
  return 'stopped'
}

function initUiState(defs: ServiceDef[]): void {
  const next: Record<string, ServiceUiState> = {}
  for (const def of defs) {
    next[def.id] = {
      id: def.id,
      selected: uiState.value[def.id]?.selected ?? false,
      status: uiState.value[def.id]?.status ?? deriveStatus(def.id),
      memMb: uiState.value[def.id]?.memMb ?? null,
    }
  }
  uiState.value = next
}

function syncStatuses(): void {
  for (const id of Object.keys(uiState.value)) {
    const current = uiState.value[id]
    const derived = deriveStatus(id)
    if (current.status !== 'starting' && current.status !== 'stopping') {
      // Pesan gagal disimpan sampai service berjalan atau dicoba lagi.
      uiState.value[id] = { ...current, status: derived, error: derived === 'running' ? undefined : current.error }
    }
  }
}

function applyStatusChange(id: string, running: boolean): void {
  const defExists = definitions.value.some((d) => d.id === id)
  if (!defExists) return

  const idx = statuses.value.findIndex((s) => s.id === id)
  if (idx >= 0) {
    statuses.value[idx] = { ...statuses.value[idx], running, state: running ? 'running' : 'exited' }
  } else {
    statuses.value.push({ id, containerName: `servel_${id}`, running, state: running ? 'running' : 'exited', exitCode: running ? null : 0 })
  }

  // Update uiState — resolve transitional state hanya saat polling result match target end-state.
  // 'starting' tahan sampai running=true; 'stopping' tahan sampai running=false.
  const current = uiState.value[id]
  if (current) {
    const holdTransition =
      (current.status === 'starting' && !running) ||
      (current.status === 'stopping' && running)
    if (!holdTransition) {
      uiState.value[id] = {
        ...current,
        status: running ? 'running' : 'stopped',
        error: running ? undefined : current.error,
        // Continuous reconcile: container yang terobservasi running → switch ON.
        // Jaga selectedIds selalu ⊇ running supaya `start` mengirim full set dan
        // --remove-orphans (backend) tak menghancurkan yang running. Tidak
        // meng-deselect saat stop — `selected` = keinginan terakhir user.
        selected: running ? true : current.selected,
        memMb: running ? current.memMb : null,
      }
    }
  }
}

/** Kapan perintah start/stop terakhir untuk sebuah service selesai. */
const settledAt: Record<string, number> = {}

/** Batas menunggu status transisi setelah perintahnya selesai. Start lebih
 * lama karena service baru dihitung running setelah siap menerima koneksi. */
export const SETTLE_LIMIT_MS = { starting: 120_000, stopping: 15_000 } as const

/** Perlu diterapkan: status transisi yang sudah mencapai tujuannya, status
 * transisi yang perintahnya sudah lama selesai tetapi kenyataannya berbeda
 * (jaring pengaman), atau status tetap yang berbeda dari kenyataan. */
export function needsUpdate(
  status: ServiceUiState['status'] | undefined,
  running: boolean,
  settledFor: number | null = null,
): boolean {
  if (status === undefined) return false
  if (status === 'starting') return running || (settledFor !== null && settledFor > SETTLE_LIMIT_MS.starting)
  if (status === 'stopping') return !running || (settledFor !== null && settledFor > SETTLE_LIMIT_MS.stopping)
  return running ? status !== 'running' : status === 'running'
}

/** Paksa status mengikuti kenyataan (abaikan penahan transisi). */
function forceStatus(id: string, running: boolean): void {
  const current = uiState.value[id]
  if (!current) return
  uiState.value[id] = { ...current, status: running ? 'running' : 'stopped' }
  applyStatusChange(id, running)
}

async function registerListener(): Promise<void> {
  if (listenerRegistered) return
  listenerRegistered = true
  await listen<ContainerStatusChangedPayload>('container-status-changed', (e) => {
    applyStatusChange(e.payload.service, e.payload.running)
  })
  // Snapshot tiap putaran polling: menyelesaikan "starting"/"stopping"
  // walau perubahannya terjadi di antara dua putaran (toggle cepat).
  await listen<Record<string, boolean>>('container-status-snapshot', (e) => {
    const now = Date.now()
    for (const [id, running] of Object.entries(e.payload)) {
      const status = uiState.value[id]?.status
      const settledFor = settledAt[id] !== undefined ? now - settledAt[id] : null
      if (!needsUpdate(status, running, settledFor)) continue
      const reachedTarget = (status === 'starting' && running) || (status === 'stopping' && !running)
      if ((status === 'starting' || status === 'stopping') && !reachedTarget) forceStatus(id, running)
      else applyStatusChange(id, running)
    }
    // Service yang tidak ada di snapshot (belum pernah dibuat) = tidak berjalan.
    for (const [id, entry] of Object.entries(uiState.value)) {
      if (!(id in e.payload) && entry.status === 'stopping') applyStatusChange(id, false)
    }
  })
}

/** Waktu snapshot RAM terakhir (untuk tooltip "Measured at …"). */
export const statsAt = ref<number | null>(null)

async function registerStatsListener(): Promise<void> {
  if (statsListenerRegistered) return
  statsListenerRegistered = true
  await listen<ContainerMemStat[]>('container-stats-changed', (e) => {
    const payload = e.payload
    statsAt.value = Date.now()
    const idsInPayload = new Set<string>()
    for (const stat of payload) {
      idsInPayload.add(stat.id)
      const current = uiState.value[stat.id]
      if (current) {
        uiState.value[stat.id] = { ...current, memMb: stat.memMb }
      }
    }
    // Absent dari payload = stopped / no data → fallback ke estimate.
    for (const id of Object.keys(uiState.value)) {
      if (!idsInPayload.has(id)) {
        const current = uiState.value[id]
        if (current && current.memMb !== null) {
          uiState.value[id] = { ...current, memMb: null }
        }
      }
    }
  })
}

async function registerServicesActionListener(): Promise<void> {
  if (servicesActionListenerRegistered) return
  servicesActionListenerRegistered = true
  await listen<ServicesActionPayload>('services-action', (e) => {
    const { action, services } = e.payload
    const transitionalStatus = action === 'start' ? 'starting' : 'stopping'
    for (const id of services) {
      const current = uiState.value[id]
      // Hanya set transitional jika service tidak sedang dalam transitional state
      // yang dipicu UI — hindari double-set dari tray + UI toggle bersamaan.
      if (current && current.status !== 'starting' && current.status !== 'stopping') {
        uiState.value[id] = { ...current, status: transitionalStatus }
        delete settledAt[id]
      }
    }
  })
}

function setSelectedIds(ids: string[]): void {
  for (const id of Object.keys(uiState.value)) {
    const entry = uiState.value[id]
    if (entry) {
      uiState.value[id] = { ...entry, selected: ids.includes(id) }
    }
  }
}

// Rekonsiliasi: container yang sedang running → switch ON, di-OR dengan saved
// selection. Hanya mengubah flag `selected` (untuk tampilan + persist); TIDAK
// memicu start/stop. Panggil SETELAH config loaded + setSelectedIds.
function reconcileSelectedWithRunning(): void {
  for (const id of Object.keys(uiState.value)) {
    const entry = uiState.value[id]
    if (!entry) continue
    if (!entry.selected && entry.status === 'running') {
      uiState.value[id] = { ...entry, selected: true }
    }
  }
}

function registerPersistWatch(): void {
  if (persistWatchRegistered) return
  persistWatchRegistered = true

  const selectedIds = computed(() =>
    Object.values(uiState.value)
      .filter((s) => s.selected)
      .map((s) => s.id),
  )

  let debounceTimer: ReturnType<typeof setTimeout> | null = null
  watch(selectedIds, (newIds) => {
    // Guard: jangan persist sebelum config selesai di-load,
    // kalau tidak watch awal (selectedIds = []) akan menimpa saved selection di disk.
    if (!useConfig().loaded.value) {
      console.log('[CONFIG] skip persist, config not loaded yet')
      return
    }
    console.log('[CONFIG] selectedIds changed:', [...newIds])
    if (debounceTimer !== null) clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => {
      debounceTimer = null
      useConfig().updateSelectedServices(newIds)
    }, 500)
  })
}

export function useServices() {
  const { call } = useTauri()

  const coreServices = computed(() => definitions.value.filter((d) => d.category === 'core'))
  const additionalServices = computed(() => definitions.value.filter((d) => d.category === 'additional'))
  const customServices = computed(() => definitions.value.filter((d) => d.category === 'custom'))

  const selectedIds = computed(() =>
    Object.values(uiState.value)
      .filter((s) => s.selected)
      .map((s) => s.id),
  )

  const runningIds = computed(() =>
    Object.values(uiState.value)
      .filter((s) => s.status === 'running')
      .map((s) => s.id),
  )

  const hasStarting = computed(() =>
    Object.values(uiState.value).some((s) => s.status === 'starting'),
  )

  const hasStopping = computed(() =>
    Object.values(uiState.value).some((s) => s.status === 'stopping'),
  )

  const selectedCount = computed(() => selectedIds.value.length)
  const runningCount = computed(() => runningIds.value.length)

  const totalRamEstimateSelected = computed(() =>
    selectedIds.value.reduce((acc, id) => {
      const def = definitions.value.find((d) => d.id === id)
      return acc + (def?.ramEstimateMb ?? 0)
    }, 0),
  )

  /** Muat ulang definisi (setelah Change port atau custom service berubah). */
  async function reloadDefinitions(): Promise<void> {
    const defs = await call<ServiceDef[]>('load_services')
    if (defs) {
      definitions.value = defs
      initUiState(defs)
    }
  }

  /** Stop lalu start satu service, mis. agar port baru dipakai. */
  async function restart(id: string): Promise<void> {
    if (uiState.value[id]?.status === 'running') await stop([id])
    await start([id])
  }

  async function load(): Promise<void> {
    if (initialized) return
    initialized = true

    try {
      const [defs, stats] = await Promise.all([
        call<ServiceDef[]>('load_services'),
        call<ServiceStatus[]>('services_status').catch(() => null),
      ])

      if (defs) {
        definitions.value = defs
        if (stats) statuses.value = stats
        initUiState(defs)
      }

      await registerListener()
      await registerServicesActionListener()
      await registerStatsListener()
      registerPersistWatch()
    } catch (err) {
      initialized = false
      serviceError.value = typeof err === 'string' ? err : 'Failed to load services'
    }
  }

  function toggle(id: string): void {
    const entry = uiState.value[id]
    if (!entry) return
    const willSelect = !entry.selected
    uiState.value[id] = { ...entry, selected: willSelect }

    // Persist segera agar tray membaca Mutex Rust dengan selection terbaru,
    // tanpa menunggu debounce 500ms di registerPersistWatch.
    const newSelectedIds = Object.values(uiState.value)
      .filter((s) => s.selected)
      .map((s) => s.id)
    if (useConfig().loaded.value) {
      // Toggle = edit profil aktif: sinkronkan selection + serviceIds profil aktif.
      useConfig().applySelection(newSelectedIds)
      void useConfig().saveImmediate()
    }

    // Toggle switch langsung memicu start/stop.
    if (willSelect) {
      if (entry.status !== 'running' && entry.status !== 'starting') {
        void start([id])
      }
    } else {
      if (entry.status === 'running' || entry.status === 'starting') {
        void stop([id])
      }
    }
  }

  async function start(ids?: string[], opts?: { force?: boolean }): Promise<StartOutcome> {
    // Backend SELALU butuh full selection untuk regenerate compose file —
    // kalau cuma kirim subset, --remove-orphans bakal nge-remove container
    // yang sedang running tapi tidak ada di compose.
    const allSelected = selectedIds.value
    if (allSelected.length === 0) return { kind: 'nothing-selected' }

    // Compute "new ones" untuk UI 'starting' indicator — bisa dari `ids` arg
    // (specific toggle) atau dari semua selected yang belum running.
    const candidatePool = ids ?? allSelected
    const newOnes = candidatePool.filter((id) => {
      const current = uiState.value[id]
      return current && current.status !== 'running'
    })

    if (newOnes.length === 0) {
      useLogs('ENV').push({
        ts: new Date().toTimeString().slice(0, 8),
        src: 'ENV',
        text: 'nothing to start: all selected services are up',
      })
      return { kind: 'already-running' }
    }

    // Cek konflik host port SEBELUM start (kecuali user pilih "Tetap mulai").
    // Konflik = port dipakai proses lain (bukan container servel yang running).
    if (!opts?.force) {
      const conflicts = await call<PortConflict[]>('check_port_conflicts', { services: newOnes })
      if (conflicts && conflicts.length > 0) {
        portConflicts.value = conflicts
        for (const c of conflicts) {
          useLogs('ENV').push({
            ts: new Date().toTimeString().slice(0, 8),
            src: 'ENV',
            text: `port ${c.port} (${c.serviceName}) is used by another program`,
          })
        }
        return { kind: 'port-conflict', ids: conflicts.map((c) => c.serviceId) }
      }
    }
    portConflicts.value = []

    serviceError.value = null
    for (const id of newOnes) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'starting', error: undefined }
        delete settledAt[id]
      }
    }

    try {
      // Pass FULL selection — compose file = all selected, `up -d` idempotent
      // untuk yang sudah running, start yang belum.
      await invoke('services_start', { services: allSelected })
      for (const id of newOnes) settledAt[id] = Date.now()
      return { kind: 'started', ids: newOnes }
    } catch (err) {
      const msg = typeof err === 'string' ? err : 'services_start failed'
      serviceError.value = msg
      for (const id of newOnes) {
        const current = uiState.value[id]
        if (current?.status === 'starting') {
          uiState.value[id] = { ...current, status: 'error', error: msg }
        }
      }
      return { kind: 'failed', error: msg }
    }
  }

  async function stop(ids: string[]): Promise<void> {
    if (ids.length === 0) return

    serviceError.value = null
    for (const id of ids) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'stopping', error: undefined }
        delete settledAt[id]
      }
    }

    try {
      await invoke('services_stop', { services: ids })
      for (const id of ids) settledAt[id] = Date.now()
    } catch (err) {
      const msg = typeof err === 'string' ? err : 'services_stop failed'
      serviceError.value = msg
      for (const id of ids) {
        const current = uiState.value[id]
        if (current?.status === 'stopping') {
          uiState.value[id] = { ...current, status: 'error', error: msg }
        }
      }
    }
  }

  // Terapkan profil: set switch (uiState) sesuai serviceIds profil, jadikan aktif,
  // dan persist segera agar tray baca selection terbaru. Tidak meng-start/stop
  // container yang sedang jalan — hanya mengubah selection (desired state).
  function applyProfile(id: string): void {
    const cfg = useConfig()
    const p = cfg.config.value.profiles.find((pr) => pr.id === id)
    if (!p) return
    setSelectedIds(p.serviceIds)
    cfg.setActiveProfile(id)
    void cfg.saveImmediate()
  }

  function dismissPortConflicts(): void {
    portConflicts.value = []
  }

  // "Tetap mulai" dari banner konflik: start service yang tadi diblok, lewati probe.
  async function startIgnoringConflicts(): Promise<void> {
    const ids = portConflicts.value.map((c) => c.serviceId)
    portConflicts.value = []
    if (ids.length > 0) await start(ids, { force: true })
  }

  async function stopAll(): Promise<void> {
    serviceError.value = null
    const runningSnapshot = [...runningIds.value]
    for (const id of runningSnapshot) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'stopping' }
        delete settledAt[id]
      }
    }

    try {
      await invoke('services_stop_all')
      for (const id of runningSnapshot) settledAt[id] = Date.now()
      if (runningSnapshot.length > 0) {
        useLogs('ENV').push({
          ts: new Date().toTimeString().slice(0, 8),
          src: 'ENV',
          text: `stopped ${runningSnapshot.length} service${runningSnapshot.length > 1 ? 's' : ''}, selection kept`,
        })
      }
    } catch (err) {
      const msg = typeof err === 'string' ? err : 'services_stop_all failed'
      serviceError.value = msg
      for (const id of Object.keys(uiState.value)) {
        const current = uiState.value[id]
        if (current?.status === 'stopping') {
          uiState.value[id] = { ...current, status: 'error', error: msg }
        }
      }
    }
  }

  return {
    definitions,
    statuses,
    uiState,
    serviceError,
    portConflicts,
    coreServices,
    additionalServices,
    customServices,
    selectedIds,
    runningIds,
    hasStarting,
    hasStopping,
    selectedCount,
    runningCount,
    totalRamEstimateSelected,
    load,
    toggle,
    syncStatuses,
    setSelectedIds,
    reconcileSelectedWithRunning,
    applyProfile,
    start,
    stop,
    stopAll,
    dismissPortConflicts,
    startIgnoringConflicts,
    reloadDefinitions,
    restart,
  }
}
