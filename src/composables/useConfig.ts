import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { EnginePreference } from '@/types/prereq'
import type { Project, ProjectGroup } from '@/types/mise'

export interface Profile {
  id: string
  name: string
  serviceIds: string[]
}

export interface ConfigState {
  version: number
  selectedServiceIds: string[]
  profiles: Profile[]
  activeProfileId: string | null
  lastPhpVersion: string | null
  lastNodeVersion: string | null
  watchedPath: string | null
  autoStart: boolean
  /** Start projects marked "start on launch" when Kova opens. */
  autoStartProjects: boolean
  rememberSession: boolean
  minimizeToTray: boolean
  /** Skala antarmuka (zoom webview). */
  uiScale?: number
  // Engine container: 'auto' | 'docker' | 'wslc' (sejak config v5).
  containerRuntime: EnginePreference
  /** Field config v6; tidak dibaca lagi (mise satu-satunya), disimpan agar config tetap utuh. */
  versionProvider: 'auto' | 'mise' | 'legacy'
  projects: Project[]
  /** Kelompok project dalam urutan tampil. Config lama tanpa field ini = tanpa kelompok. */
  groups?: ProjectGroup[]
  // Project aktif untuk konteks versi mise; null = Global.
  activeProjectId: string | null
  /** Quit Kova juga menghentikan service. */
  stopServicesOnQuit: boolean
  /** Onboarding sudah dilewati sekali. */
  onboarded: boolean
  /** Services › Change port: port di PC per service id. */
  servicePorts?: Record<string, number>
}

const DEFAULT_CONFIG: ConfigState = {
  version: 7,
  selectedServiceIds: [],
  profiles: [{ id: 'default', name: 'Default', serviceIds: [] }],
  activeProfileId: 'default',
  lastPhpVersion: null,
  lastNodeVersion: null,
  watchedPath: null,
  autoStart: false,
  autoStartProjects: true,
  rememberSession: true,
  minimizeToTray: true,
  uiScale: 1,
  containerRuntime: 'auto',
  versionProvider: 'auto',
  projects: [],
  groups: [],
  activeProjectId: null,
  stopServicesOnQuit: true,
  onboarded: false,
}

// Singleton state
const config = ref<ConfigState>(structuredClone(DEFAULT_CONFIG))
const loaded = ref(false)
/** Pesan bila config.json tidak terbaca; kosong bila baik. */
const loadError = ref('')

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
    let result: ConfigState
    try {
      result = await invoke<ConfigState>('config_read')
    } catch (err) {
      loadError.value = String(err)
      throw err
    }
    loadError.value = ''
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

  function setAutoStartProjects(value: boolean): void {
    config.value.autoStartProjects = value
    scheduleSave()
  }

  /** Port di PC untuk satu service; `null` = kembali ke port bawaan. Disimpan langsung. */
  async function setServicePort(id: string, port: number | null): Promise<void> {
    const next = { ...(config.value.servicePorts ?? {}) }
    if (port === null) delete next[id]
    else next[id] = port
    config.value.servicePorts = next
    await saveImmediate()
  }

  function setRememberSession(value: boolean): void {
    config.value.rememberSession = value
    scheduleSave()
  }

  function setStopServicesOnQuit(value: boolean): void {
    config.value.stopServicesOnQuit = value
    scheduleSave()
  }

  /** Tandai onboarding selesai; disimpan langsung supaya tidak hilang bila Kova segera ditutup. */
  async function markOnboarded(): Promise<void> {
    if (config.value.onboarded) return
    config.value.onboarded = true
    await writeNow()
  }

  function setUiScale(value: number): void {
    config.value.uiScale = value
    scheduleSave()
  }

  function setMinimizeToTray(value: boolean): void {
    config.value.minimizeToTray = value
    scheduleSave()
  }

  async function writeNow(): Promise<void> {
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    await invoke<void>('config_write', { config: config.value })
  }

  // Project & konteks versi ditulis segera: backend mise membaca config dari
  // disk untuk menentukan cwd, jadi debounce akan membuat konteks tertinggal.
  async function addProject(path: string, extra: Partial<Project> = {}): Promise<Project | null> {
    const clean = path.replace(/[\\/]+$/, '')
    if (!clean) return null
    const exists = config.value.projects.find((p) => p.path.toLowerCase() === clean.toLowerCase())
    if (exists) return exists
    const name = clean.split(/[\\/]/).pop() || clean
    const project: Project = { id: crypto.randomUUID(), name, ...extra, path: clean }
    config.value.projects = [...config.value.projects, project]
    await writeNow()
    return project
  }

  async function removeProject(id: string): Promise<void> {
    config.value.projects = config.value.projects.filter((p) => p.id !== id)
    if (config.value.activeProjectId === id) config.value.activeProjectId = null
    await writeNow()
  }

  async function setActiveProject(id: string | null): Promise<void> {
    config.value.activeProjectId = id
    await writeNow()
  }

  async function updateProject(id: string, patch: Partial<Project>): Promise<void> {
    config.value.projects = config.value.projects.map((p) => (p.id === id ? { ...p, ...patch, id } : p))
    await writeNow()
  }

  // Kelompok dan urutan ditulis segera: popup tray dan menu Linux membaca
  // config dari disk.
  async function createGroup(name: string, projectId?: string): Promise<ProjectGroup> {
    const group: ProjectGroup = { id: crypto.randomUUID(), name: name.trim(), collapsed: false }
    config.value.groups = [...(config.value.groups ?? []), group]
    if (projectId) {
      const moving = config.value.projects.find((p) => p.id === projectId)
      if (moving) config.value.projects = [...config.value.projects.filter((p) => p.id !== projectId), { ...moving, groupId: group.id }]
    }
    await writeNow()
    return group
  }

  async function renameGroup(id: string, name: string): Promise<void> {
    config.value.groups = (config.value.groups ?? []).map((g) => (g.id === id ? { ...g, name: name.trim() } : g))
    await writeNow()
  }

  /** Hapus kelompok; project di dalamnya pindah ke Ungrouped. */
  async function deleteGroup(id: string): Promise<void> {
    config.value.groups = (config.value.groups ?? []).filter((g) => g.id !== id)
    config.value.projects = config.value.projects.map((p) => (p.groupId === id ? { ...p, groupId: null } : p))
    await writeNow()
  }

  function setGroupCollapsed(id: string, collapsed: boolean): void {
    config.value.groups = (config.value.groups ?? []).map((g) => (g.id === id ? { ...g, collapsed } : g))
    scheduleSave()
  }

  async function setProjects(projects: Project[]): Promise<void> {
    config.value.projects = projects
    await writeNow()
  }

  async function setProjectProfile(projectId: string, profileId: string | null): Promise<void> {
    config.value.projects = config.value.projects.map((p) =>
      p.id === projectId ? { ...p, profileId } : p,
    )
    await writeNow()
  }

  // Tulis segera (bukan debounce): backend me-resolve ulang engine saat
  // config_write, dan caller langsung re-probe status sesudahnya.
  async function setContainerRuntime(value: EnginePreference): Promise<void> {
    config.value.containerRuntime = value
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    await invoke<void>('config_write', { config: config.value })
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
    loadError,
    load,
    save,
    saveImmediate,
    scheduleSave,
    setAutoStart,
    setAutoStartProjects,
    setServicePort,
    setRememberSession,
    setMinimizeToTray,
    setStopServicesOnQuit,
    markOnboarded,
    setContainerRuntime,
    setUiScale,
    addProject,
    removeProject,
    updateProject,
    setActiveProject,
    setProjectProfile,
    createGroup,
    renameGroup,
    deleteGroup,
    setGroupCollapsed,
    setProjects,
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
