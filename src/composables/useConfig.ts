import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface ConfigState {
  version: number
  selectedServiceIds: string[]
  lastPhpVersion: string | null
  lastNodeVersion: string | null
  watchedPath: string | null
  autoStart: boolean
  rememberSession: boolean
  minimizeToTray: boolean
}

const DEFAULT_CONFIG: ConfigState = {
  version: 1,
  selectedServiceIds: [],
  lastPhpVersion: null,
  lastNodeVersion: null,
  watchedPath: null,
  autoStart: false,
  rememberSession: true,
  minimizeToTray: true,
}

// Singleton state
const config = ref<ConfigState>({ ...DEFAULT_CONFIG })
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

  function updateSelectedServices(ids: string[]): void {
    if (!config.value.rememberSession) return
    config.value.selectedServiceIds = ids
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
    const def: ConfigState = { ...DEFAULT_CONFIG }
    config.value = def
    await invoke<void>('config_write', { config: def })
  }

  return {
    config,
    loaded,
    load,
    save,
    saveImmediate,
    setAutoStart,
    setRememberSession,
    setMinimizeToTray,
    updateSelectedServices,
    setLastPhpVersion,
    setLastNodeVersion,
    reset,
  }
}
