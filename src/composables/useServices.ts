import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useTauri } from './useTauri'
import type { ServiceDef, ServiceStatus, ServiceUiState } from '@/types/service'

interface ContainerStatusChangedPayload {
  service: string
  running: boolean
}

// Singleton state — shared across all consumers
const definitions = ref<ServiceDef[]>([])
const statuses = ref<ServiceStatus[]>([])
const uiState = ref<Record<string, ServiceUiState>>({})
const serviceError = ref<string | null>(null)
let initialized = false
let listenerRegistered = false

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

  // Update uiState — always reflect event result (transitional states resolve here)
  const current = uiState.value[id]
  if (current) {
    uiState.value[id] = { ...current, status: running ? 'running' : 'stopped' }
  }
}

async function registerListener(): Promise<void> {
  if (listenerRegistered) return
  listenerRegistered = true
  await listen<ContainerStatusChangedPayload>('container-status-changed', (e) => {
    applyStatusChange(e.payload.service, e.payload.running)
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
    } catch (err) {
      initialized = false
      serviceError.value = typeof err === 'string' ? err : 'Failed to load services'
    }
  }

  function toggle(id: string): void {
    const entry = uiState.value[id]
    if (!entry) return
    uiState.value[id] = { ...entry, selected: !entry.selected }
  }

  async function start(ids?: string[]): Promise<void> {
    const targets = ids ?? selectedIds.value
    if (targets.length === 0) return

    serviceError.value = null
    for (const id of targets) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'starting' }
      }
    }

    try {
      await invoke('services_start', { services: targets })
    } catch (err) {
      const msg = typeof err === 'string' ? err : 'services_start gagal'
      serviceError.value = msg
      for (const id of targets) {
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
    for (const id of runningIds.value) {
      if (uiState.value[id]) {
        uiState.value[id] = { ...uiState.value[id], status: 'stopping' }
      }
    }

    try {
      await invoke('services_stop_all')
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
    start,
    stop,
    stopAll,
  }
}
