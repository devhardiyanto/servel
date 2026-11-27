import type { ServiceDef } from '@/types/service'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { samePath } from '@/utils/paths'
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
  /** Notifikasi OS saat project, proses, atau service berhenti sendiri. */
  notifications?: boolean
  /** Zoom webview. */
  uiScale?: number
  /** Sejak config v5. */
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
  onboarded: boolean
  /** Services › Change port: port di PC per service id. */
  servicePorts?: Record<string, number>
  /** Services › Add service. */
  customServices?: ServiceDef[]
  /** Runtimes › Add tool: tool mise tambahan (bun, rust, java…). */
  extraTools?: string[]
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
  notifications: true,
  uiScale: 1,
  containerRuntime: 'auto',
  versionProvider: 'auto',
  projects: [],
  groups: [],
  activeProjectId: null,
  stopServicesOnQuit: true,
  onboarded: false,
}

const config = ref<ConfigState>(structuredClone(DEFAULT_CONFIG))
const loaded = ref(false)
/** Pesan bila config.json tidak terbaca; kosong bila baik. */
const loadError = ref('')

let saveTimer: ReturnType<typeof setTimeout> | null = null

// Isi config tidak dicetak ke console: memuat nilai Environment (password, kunci API).
/**
 * Salinan lepas dari config. Bukan `structuredClone`: objek di dalam config bisa
 * berupa proxy reaktif Vue (mis. hasil `envs.map(...)`), dan proxy tidak bisa di-clone.
 * Config selalu JSON (itu yang ditulis ke disk), jadi salinan JSON setara.
 */
function cloneConfig(c: ConfigState): ConfigState {
  return JSON.parse(JSON.stringify(c)) as ConfigState
}

/** Isi terakhir yang berhasil ditulis atau dibaca; dipakai bila simpan gagal. */
let lastSaved: ConfigState | null = null
const saveFailed = ref('')

async function writeConfig(): Promise<void> {
  const snapshot = cloneConfig(config.value)
  await invoke<void>('config_write', { config: config.value })
  lastSaved = snapshot
}

function scheduleSave(): void {
  if (saveTimer !== null) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    saveTimer = null
    const snapshot = cloneConfig(config.value)
    invoke<void>('config_write', { config: config.value })
      .then(() => (lastSaved = snapshot))
      .catch((err) => {
        // Canvas S-Settings › Error: toggle kembali ke nilai tersimpan dan pesan tampil.
        if (lastSaved) config.value = cloneConfig(lastSaved)
        saveFailed.value = String(err)
      })
  }, 500)
}

/**
 * Ubah config lalu tulis segera. Bila penulisan gagal, memori dikembalikan
 * ke isi sebelumnya dan error diteruskan, supaya UI tidak mengira tersimpan.
 */
async function commit(mutate: () => void, write: () => Promise<void>): Promise<void> {
  const before = cloneConfig(config.value)
  mutate()
  try {
    await write()
  } catch (err) {
    config.value = before
    throw err
  }
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
    lastSaved = cloneConfig(result)
    loaded.value = true
  }

  async function save(): Promise<void> {
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    await writeConfig()
  }

  // Tray membaca config dari backend: toggle tidak boleh menunggu debounce 500 ms.
  async function saveImmediate(): Promise<void> {
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    await writeConfig()
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
    await commit(() => (config.value.servicePorts = next), saveImmediate)
  }

  /** Tambah atau ganti custom service (id sama = ganti). Disimpan langsung. */
  async function saveCustomService(def: ServiceDef): Promise<void> {
    const list = (config.value.customServices ?? []).filter((d) => d.id !== def.id)
    await commit(() => (config.value.customServices = [...list, def]), saveImmediate)
  }

  async function addExtraTool(tool: string): Promise<void> {
    const list = config.value.extraTools ?? []
    if (!list.includes(tool)) config.value.extraTools = [...list, tool]
    await saveImmediate()
  }

  async function removeExtraTool(tool: string): Promise<void> {
    config.value.extraTools = (config.value.extraTools ?? []).filter((t) => t !== tool)
    await saveImmediate()
  }

  async function removeCustomService(id: string): Promise<void> {
    config.value.customServices = (config.value.customServices ?? []).filter((d) => d.id !== id)
    for (const p of config.value.profiles) p.serviceIds = p.serviceIds.filter((x) => x !== id)
    if (config.value.servicePorts?.[id] != null) {
      const next = { ...config.value.servicePorts }
      delete next[id]
      config.value.servicePorts = next
    }
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

  function setNotifications(value: boolean): void {
    config.value.notifications = value
    scheduleSave()
  }

  async function writeNow(): Promise<void> {
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    await writeConfig()
  }

  // Project & konteks versi ditulis segera: backend mise membaca config dari
  // disk untuk menentukan cwd, jadi debounce akan membuat konteks tertinggal.
  async function addProject(path: string, extra: Partial<Project> = {}): Promise<Project | null> {
    const clean = path.replace(/[\\/]+$/, '')
    if (!clean) return null
    const exists = config.value.projects.find((p) => samePath(p.path, clean))
    if (exists) return exists
    const name = clean.split(/[\\/]/).pop() || clean
    const project: Project = { id: crypto.randomUUID(), name, ...extra, path: clean }
    await commit(() => (config.value.projects = [...config.value.projects, project]), writeNow)
    return project
  }

  async function removeProject(id: string): Promise<void> {
    await commit(() => {
      config.value.projects = config.value.projects.filter((p) => p.id !== id)
      if (config.value.activeProjectId === id) config.value.activeProjectId = null
    }, writeNow)
  }

  async function setActiveProject(id: string | null): Promise<void> {
    config.value.activeProjectId = id
    await writeNow()
  }

  async function updateProject(id: string, patch: Partial<Project>): Promise<void> {
    await commit(() => (config.value.projects = config.value.projects.map((p) => (p.id === id ? { ...p, ...patch, id } : p))), writeNow)
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
    await writeConfig()
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

  // Profil terakhir tidak bisa dihapus. Menghapus profil aktif memindahkan pilihan ke
  // profil pertama yang tersisa; hasilnya id profil aktif yang baru.
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
    lastSaved = cloneConfig(def)
  }

  return {
    saveFailed,
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
    saveCustomService,
    addExtraTool,
    removeExtraTool,
    removeCustomService,
    setRememberSession,
    setMinimizeToTray,
    setNotifications,
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
