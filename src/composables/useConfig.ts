import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Profile {
  id: string
  name: string
  serviceIds: string[]
}

// Domain lokal → IP via file hosts (fitur Sites, M11). Hosts = proyeksi sites
// yang enabled; config.json = source of truth. Apply ke hosts bersifat manual.
// Tujuan routing site di balik reverse proxy (mirror `SiteTarget` Rust).
// Tagged supaya varian lain bisa menyusul tanpa migrasi config ulang.
export type SiteTarget = { kind: 'port'; value: number }

export interface Site {
  id: string
  domain: string
  ip: string
  enabled: boolean
  // null = site hosts-only: nama resolve, tapi tak ada yang mem-proxy-kan.
  target?: SiteTarget | null
}

export interface ConfigState {
  version: number
  selectedServiceIds: string[]
  profiles: Profile[]
  activeProfileId: string | null
  sites: Site[]
  lastPhpVersion: string | null
  lastNodeVersion: string | null
  watchedPath: string | null
  autoStart: boolean
  rememberSession: boolean
  minimizeToTray: boolean
}

const DEFAULT_CONFIG: ConfigState = {
  version: 3,
  selectedServiceIds: [],
  profiles: [{ id: 'default', name: 'Default', serviceIds: [] }],
  activeProfileId: 'default',
  sites: [],
  lastPhpVersion: null,
  lastNodeVersion: null,
  watchedPath: null,
  autoStart: false,
  rememberSession: true,
  minimizeToTray: true,
}

// Singleton state
const config = ref<ConfigState>(structuredClone(DEFAULT_CONFIG))
const loaded = ref(false)

let saveTimer: ReturnType<typeof setTimeout> | null = null

function scheduleSave(): void {
  console.log('[CONFIG] schedule save:', JSON.parse(JSON.stringify(config.value)))
  if (saveTimer !== null) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    saveTimer = null
    invoke<void>('config_write', { config: config.value })
      .then(() => console.log('[CONFIG] write OK'))
      .catch((err) => {
        console.error('[CONFIG] write FAILED:', err)
      })
  }, 500)
}

export function useConfig() {
  async function load(): Promise<void> {
    const result = await invoke<ConfigState>('config_read')
    config.value = result
    loaded.value = true
    console.log('[CONFIG] loaded:', JSON.parse(JSON.stringify(result)))
  }

  async function save(): Promise<void> {
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    await invoke<void>('config_write', { config: config.value })
  }

  // Flush pending debounce + write immediately — dipakai oleh toggle path
  // agar tray baca Mutex Rust dengan nilai terbaru sebelum 500ms debounce expired.
  async function saveImmediate(): Promise<void> {
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    await invoke<void>('config_write', { config: config.value })
      .then(() => console.log('[CONFIG] write immediate OK'))
      .catch((err) => console.error('[CONFIG] write immediate FAILED:', err))
  }

  function setAutoStart(value: boolean): void {
    config.value.autoStart = value
    scheduleSave()
  }

  function setRememberSession(value: boolean): void {
    config.value.rememberSession = value
    scheduleSave()
  }

  function setMinimizeToTray(value: boolean): void {
    config.value.minimizeToTray = value
    scheduleSave()
  }

  // Selection = proyeksi profil aktif. Set selectedServiceIds DAN serviceIds
  // profil aktif dalam satu langkah, agar konsisten dgn re-proyeksi backend
  // (yang menurunkan selectedServiceIds dari profil aktif saat write).
  function syncSelection(ids: string[]): void {
    config.value.selectedServiceIds = [...ids]
    const activeId = config.value.activeProfileId
    if (activeId) {
      const p = config.value.profiles.find((pr) => pr.id === activeId)
      if (p) p.serviceIds = [...ids]
    }
  }

  function updateSelectedServices(ids: string[]): void {
    if (!config.value.rememberSession) return
    syncSelection(ids)
    scheduleSave()
  }

  // Dipakai path toggle (immediate persist, bypass rememberSession) — tetap
  // meng-edit profil aktif ("toggle = edit profil aktif").
  function applySelection(ids: string[]): void {
    syncSelection(ids)
  }

  function createProfile(name: string, serviceIds: string[] = []): string {
    const id = crypto.randomUUID()
    config.value.profiles.push({ id, name, serviceIds: [...serviceIds] })
    scheduleSave()
    return id
  }

  function renameProfile(id: string, name: string): void {
    const p = config.value.profiles.find((pr) => pr.id === id)
    if (!p) return
    p.name = name
    scheduleSave()
  }

  // Guard: minimal selalu ada 1 profil. Hapus profil aktif → fallback ke profil
  // pertama tersisa (selectedServiceIds ikut). Return id aktif baru (utk sinkron UI).
  function deleteProfile(id: string): string | null {
    if (config.value.profiles.length <= 1) return config.value.activeProfileId
    const idx = config.value.profiles.findIndex((pr) => pr.id === id)
    if (idx < 0) return config.value.activeProfileId
    config.value.profiles.splice(idx, 1)
    if (config.value.activeProfileId === id) {
      const fallback = config.value.profiles[0]
      config.value.activeProfileId = fallback.id
      config.value.selectedServiceIds = [...fallback.serviceIds]
    }
    scheduleSave()
    return config.value.activeProfileId
  }

  // Set profil aktif + turunkan selectedServiceIds dari serviceIds-nya. Sinkron
  // ke uiState (switch) ditangani useServices.applyProfile.
  function setActiveProfile(id: string): void {
    const p = config.value.profiles.find((pr) => pr.id === id)
    if (!p) return
    config.value.activeProfileId = id
    config.value.selectedServiceIds = [...p.serviceIds]
    scheduleSave()
  }

  function setLastPhpVersion(version: string | null): void {
    config.value.lastPhpVersion = version
    scheduleSave()
  }

  function setLastNodeVersion(version: string | null): void {
    config.value.lastNodeVersion = version
    scheduleSave()
  }

  async function reset(): Promise<void> {
    const def: ConfigState = structuredClone(DEFAULT_CONFIG)
    config.value = def
    await invoke<void>('config_write', { config: def })
  }

  return {
    config,
    loaded,
    load,
    save,
    saveImmediate,
    scheduleSave,
    setAutoStart,
    setRememberSession,
    setMinimizeToTray,
    updateSelectedServices,
    applySelection,
    createProfile,
    renameProfile,
    deleteProfile,
    setActiveProfile,
    setLastPhpVersion,
    setLastNodeVersion,
    reset,
  }
}
