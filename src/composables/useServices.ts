import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useTauri } from './useTauri'
import { useConfig } from './useConfig'
import { useLogs } from './useLogs'
import type { ServiceDef, ServiceStatus, ServiceUiState } from '@/types/service'

interface ContainerStatusChangedPayload {
  service: string
  running: boolean
}

interface ServicesActionPayload {
  action: 'start' | 'stop_all'
  services: string[]
}

// Singleton state — shared across all consumers
const definitions = ref<ServiceDef[]>([])
const statuses = ref<ServiceStatus[]>([])
const uiState = ref<Record<string, ServiceUiState>>({})
const serviceError = ref<string | null>(null)
let initialized = false
let listenerRegistered = false
let servicesActionListenerRegistered = false
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
    }
  }
  uiState.value = next
}

function syncStatuses(): void {
  for (const id of Object.keys(uiState.value)) {
    const current = uiState.value[id]
    const derived = deriveStatus(id)
    if (current.status !== 'starting' && current.status !== 'stopping') {
      uiState.value[id] = { ...current, status: derived }
    }
  }
}

function applyStatusChange(id: string, running: boolean): void {
  // Ignore events for ids not in definitions
  const defExists = definitions.value.some((d) => d.id === id)
  if (!defExists) return

  // Update statuses array
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
      uiState.value[id] = { ...current, status: running ? 'running' : 'stopped' }
    }
  }
}

async function registerListener(): Promise<void> {
  if (listenerRegistered) return
  listenerRegistered = true
  await listen<ContainerStatusChangedPayload>('container-status-changed', (e) => {
    applyStatusChange(e.payload.service, e.payload.running)
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
      console.log('[CONFIG] skip persist — config not loaded yet')
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

    // Immediate persist agar tray baca Mutex Rust dengan selection terbaru —
    // bypass debounce 500ms di registerPersistWatch (race fix sub-bug 2b).
    const newSelectedIds = Object.values(uiState.value)
      .filter((s) => s.selected)
      .map((s) => s.id)
    if (useConfig().loaded.value) {
      useConfig().config.value.selectedServiceIds = newSelectedIds
      void useConfig().saveImmediate()
    }

    // Auto-action per servel-app.jsx pattern: toggle switch = trigger start/stop
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

  async function start(ids?: string[]): Promise<void> {
    // Backend SELALU butuh full selection untuk regenerate compose file —
    // kalau cuma kirim subset, --remove-orphans bakal nge-remove container
    // yang sedang running tapi tidak ada di compose (bug stuck loading sebelumnya).
    const allSelected = selectedIds.value
    if (allSelected.length === 0) return

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
        text: 'nothing to start — all selected services are up',
      })
      return
    }

    serviceError.value = null
    for (const id of newOnes) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'starting' }
      }
    }

    try {
      // Pass FULL selection — compose file = all selected, `up -d` idempotent
      // untuk yang sudah running, start yang belum.
      await invoke('services_start', { services: allSelected })
    } catch (err) {
      const msg = typeof err === 'string' ? err : 'services_start gagal'
      serviceError.value = msg
      for (const id of newOnes) {
        const current = uiState.value[id]
        if (current?.status === 'starting') {
          uiState.value[id] = { ...current, status: 'error' }
        }
      }
    }
  }

  async function stop(ids: string[]): Promise<void> {
    if (ids.length === 0) return

    serviceError.value = null
    for (const id of ids) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'stopping' }
      }
    }

    try {
      await invoke('services_stop', { services: ids })
    } catch (err) {
      const msg = typeof err === 'string' ? err : 'services_stop gagal'
      serviceError.value = msg
      for (const id of ids) {
        const current = uiState.value[id]
        if (current?.status === 'stopping') {
          uiState.value[id] = { ...current, status: 'error' }
        }
      }
    }
  }

  async function stopAll(): Promise<void> {
    serviceError.value = null
    const runningSnapshot = [...runningIds.value]
    for (const id of runningSnapshot) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'stopping' }
      }
    }

    try {
      await invoke('services_stop_all')
      if (runningSnapshot.length > 0) {
        useLogs('ENV').push({
          ts: new Date().toTimeString().slice(0, 8),
          src: 'ENV',
          text: `stopped ${runningSnapshot.length} service${runningSnapshot.length > 1 ? 's' : ''} — selection kept`,
        })
      }
    } catch (err) {
      const msg = typeof err === 'string' ? err : 'services_stop_all gagal'
      serviceError.value = msg
      for (const id of Object.keys(uiState.value)) {
        const current = uiState.value[id]
        if (current?.status === 'stopping') {
          uiState.value[id] = { ...current, status: 'error' }
        }
      }
    }
  }

  return {
    definitions,
    statuses,
    uiState,
    serviceError,
    coreServices,
    additionalServices,
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
    start,
    stop,
    stopAll,
  }
}
