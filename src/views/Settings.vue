<script setup lang="ts">
import { ref, computed, inject, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { useVersionContext } from '@/composables/useVersionContext'
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import { useUpdateCheck } from '@/composables/useUpdateCheck'
import {
  useSites,
  isValidIpv4,
  isValidDomain,
  parsePort,
  type SitesStatus,
  type DiffResult,
} from '@/composables/useSites'
import { useProxy } from '@/composables/useProxy'
import SettingRow from '@/components/SettingRow.vue'
import { ENGINE_LABEL, type EnginePreference, type PrereqStatus } from '@/types/prereq'
import { useEngineStatus } from '@/composables/useEngineStatus'

type SettingsNav = 'general' | 'services' | 'sites' | 'php_node' | 'about'

const setView = inject(SetViewKey)!

const {
  config,
  loaded,
  load,
  saveImmediate,
  setAutoStart,
  setRememberSession,
  setMinimizeToTray,
  setContainerRuntime,
  addProject,
  removeProject,
  setVersionProvider,
  setProjectProfile,
  createProfile,
  renameProfile,
  deleteProfile,
  reset,
} = useConfig()

const activeNav = ref<SettingsNav>('general')

// --- Profiles CRUD ---
const newProfileName = ref('')
const editingId = ref<string | null>(null)
const editingName = ref('')

function handleCreateProfile(): void {
  const name = newProfileName.value.trim()
  if (!name) return
  // Snapshot selection saat ini sebagai preset awal profil baru.
  createProfile(name, [...config.value.selectedServiceIds])
  newProfileName.value = ''
}

function startEditProfile(id: string, name: string): void {
  editingId.value = id
  editingName.value = name
}

function commitEditProfile(): void {
  const name = editingName.value.trim()
  if (editingId.value && name) renameProfile(editingId.value, name)
  editingId.value = null
  editingName.value = ''
}

function cancelEditProfile(): void {
  editingId.value = null
  editingName.value = ''
}

async function handleDeleteProfile(id: string, name: string): Promise<void> {
  if (config.value.profiles.length <= 1) return
  let ok = false
  try {
    ok = await ask(`Hapus profil "${name}"?`, { title: 'Hapus Profil', kind: 'warning' })
  } catch {
    ok = window.confirm(`Hapus profil "${name}"?`)
  }
  if (!ok) return
  deleteProfile(id)
}

// --- Sites (hosts) ---
const {
  sites,
  addSite,
  updateSite,
  deleteSite,
  toggleSite,
  status: fetchSitesStatus,
  apply: applySites,
  restore: restoreSites,
} = useSites()

const newDomain = ref('')
const newIp = ref('127.0.0.1')
const newPort = ref('')
const siteError = ref('')
const editingSiteId = ref<string | null>(null)
const editDomain = ref('')
const editIp = ref('')
const editPort = ref('')

const sitesStatus = ref<SitesStatus | null>(null)
const sitesBusy = ref(false) // apply/restore sedang jalan (UAC)
const applyError = ref('')
const showDiffModal = ref(false)
const pendingDiff = ref<DiffResult | null>(null)
const selectedBackup = ref('')

async function refreshSitesStatus(): Promise<void> {
  // Flush debounce config dulu: sites_status baca config dari disk, jadi mutasi
  // (add/edit/delete/toggle) harus ter-tulis sebelum status dibaca (hindari race).
  await saveImmediate()
  sitesStatus.value = await fetchSitesStatus()
}

function handleAddSite(): void {
  const d = newDomain.value.trim()
  const ip = newIp.value.trim() || '127.0.0.1'
  siteError.value = ''
  if (!isValidDomain(d)) {
    siteError.value = 'Domain tidak valid'
    return
  }
  if (!isValidIpv4(ip)) {
    siteError.value = 'IP tidak valid'
    return
  }
  if (sites.value.some((s) => s.domain === d)) {
    siteError.value = 'Domain sudah terdaftar'
    return
  }
  const target = parsePort(newPort.value)
  if (newPort.value.trim() !== '' && target === null) {
    siteError.value = 'Port tujuan harus angka 1–65535'
    return
  }
  addSite(d, ip, target)
  newDomain.value = ''
  newIp.value = '127.0.0.1'
  newPort.value = ''
  void refreshSitesStatus()
  void applyProxyChanges()
}

function startEditSite(id: string, domain: string, ip: string, port: number | null): void {
  editingSiteId.value = id
  editDomain.value = domain
  editIp.value = ip
  editPort.value = port === null ? '' : String(port)
  siteError.value = ''
}

function commitEditSite(): void {
  const d = editDomain.value.trim()
  const ip = editIp.value.trim() || '127.0.0.1'
  siteError.value = ''
  if (!isValidDomain(d) || !isValidIpv4(ip)) {
    siteError.value = 'Domain atau IP tidak valid'
    return
  }
  const target = parsePort(editPort.value)
  if (editPort.value.trim() !== '' && target === null) {
    siteError.value = 'Port tujuan harus angka 1–65535'
    return
  }
  if (editingSiteId.value) updateSite(editingSiteId.value, { domain: d, ip, target })
  editingSiteId.value = null
  void refreshSitesStatus()
  void applyProxyChanges()
}

function cancelEditSite(): void {
  editingSiteId.value = null
  siteError.value = ''
}

async function handleDeleteSite(id: string, domain: string): Promise<void> {
  let ok = false
  try {
    ok = await ask(`Hapus domain "${domain}"?`, { title: 'Hapus Site', kind: 'warning' })
  } catch {
    ok = window.confirm(`Hapus domain "${domain}"?`)
  }
  if (!ok) return
  deleteSite(id)
  void refreshSitesStatus()
  void applyProxyChanges()
}

function handleToggleSite(id: string): void {
  toggleSite(id)
  void refreshSitesStatus()
  void applyProxyChanges()
}

// --- Sites (reverse proxy) ---
const {
  status: proxyStatus,
  binary: proxyBinary,
  progress: proxyProgress,
  refresh: refreshProxy,
  installBinary: installProxyBinary,
  start: startProxy,
  stop: stopProxy,
  reload: reloadProxy,
  installCert: installProxyCert,
} = useProxy()

const proxyBusy = ref(false)
const proxyError = ref('')

// Jumlah site yang layak di-proxy: aktif + punya port tujuan. Dipakai untuk
// menonaktifkan tombol start sebelum backend menolaknya.
const proxyableCount = computed(
  () => sites.value.filter((s) => s.enabled && s.target != null).length,
)

// Perubahan site harus sampai ke proxy yang sedang jalan. Config disimpan
// debounce, jadi flush dulu supaya backend membaca versi terbaru dari disk.
async function applyProxyChanges(): Promise<void> {
  if (!proxyStatus.value?.running) return
  try {
    await saveImmediate()
    await reloadProxy()
  } catch (err) {
    proxyError.value = String(err)
  }
}

async function runProxyAction(action: () => Promise<void>): Promise<void> {
  proxyBusy.value = true
  proxyError.value = ''
  try {
    await saveImmediate()
    await action()
  } catch (err) {
    proxyError.value = String(err)
  } finally {
    proxyBusy.value = false
  }
}

function handleInstallProxyBinary(): void {
  void runProxyAction(installProxyBinary)
}

function handleToggleProxy(): void {
  void runProxyAction(proxyStatus.value?.running ? stopProxy : startProxy)
}

function handleInstallCert(): void {
  void runProxyAction(installProxyCert)
}

// Buka modal diff sebelum menulis hosts (konfirmasi + UAC manual).
async function openApply(): Promise<void> {
  applyError.value = ''
  await refreshSitesStatus()
  if (!sitesStatus.value || sitesStatus.value.inSync) return
  pendingDiff.value = sitesStatus.value.diff
  showDiffModal.value = true
}

async function confirmApply(): Promise<void> {
  applyError.value = ''
  sitesBusy.value = true
  try {
    await applySites()
    showDiffModal.value = false
    await refreshSitesStatus()
  } catch (err) {
    applyError.value = String(err)
  } finally {
    sitesBusy.value = false
  }
}

function cancelApply(): void {
  showDiffModal.value = false
  applyError.value = ''
}

async function handleRestore(): Promise<void> {
  const backup = selectedBackup.value
  if (!backup) return
  let ok = false
  try {
    ok = await ask(`Pulihkan hosts dari backup ini?\n${backup}`, {
      title: 'Restore Hosts',
      kind: 'warning',
    })
  } catch {
    ok = window.confirm(`Pulihkan hosts dari backup "${backup}"?`)
  }
  if (!ok) return
  applyError.value = ''
  sitesBusy.value = true
  try {
    await restoreSites(backup)
    await refreshSitesStatus()
  } catch (err) {
    applyError.value = String(err)
  } finally {
    sitesBusy.value = false
  }
}

const {
  latestVersion,
  updateAvailable,
  checking: checkingUpdate,
  lastChecked,
  check: checkUpdate,
  openReleasePage,
} = useUpdateCheck()

const composePath = ref<string>('')
const engineInfo = ref<PrereqStatus | null>(null)
const checkingEngine = ref(false)
const switchingEngine = ref(false)
const platform = ref<string>('windows')
const version = ref<string>('')

const engineRunning = computed<boolean | null>(() => engineInfo.value?.engine_running ?? null)
const activeEngineLabel = computed<string>(() =>
  engineInfo.value ? ENGINE_LABEL[engineInfo.value.engine] : '—',
)

const ENGINE_OPTIONS: { value: EnginePreference; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  { value: 'wslc', label: 'WSL containers' },
  { value: 'docker', label: 'Docker' },
]

const { refresh: refreshEngineBanner } = useEngineStatus()

// --- Version manager (mise) & projects -------------------------------------
const versionCtx = useVersionContext()
const { ctx: miseCtx, busy: miseBusy, lastError: miseError } = versionCtx
const PROVIDER_OPTIONS: { value: 'auto' | 'mise' | 'legacy'; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  { value: 'mise', label: 'mise' },
  { value: 'legacy', label: 'phpvm + fnm' },
]

async function chooseProvider(value: 'auto' | 'mise' | 'legacy'): Promise<void> {
  if (config.value.versionProvider === value) return
  await setVersionProvider(value)
  await versionCtx.refresh()
}

async function pickProject(): Promise<void> {
  let selected: string | string[] | null = null
  try {
    selected = await openDialog({ directory: true, multiple: false, title: 'Add project folder' })
  } catch (err) {
    miseError.value = String(err)
    return
  }
  if (typeof selected === 'string') {
    await addProject(selected)
    await versionCtx.refresh()
  }
}

// --- Restore database (v1.8) ---------------------------------------------
const restoreService = ref<'mysql' | 'postgres'>('mysql')
const restoreFile = ref<string | null>(null)
const restoring = ref(false)
const restoreResult = ref<{ ok: boolean; text: string } | null>(null)

async function pickRestoreFile(): Promise<void> {
  try {
    const selected = await openDialog({
      multiple: false,
      title: 'Choose a .sql dump',
      filters: [{ name: 'SQL dump', extensions: ['sql'] }],
    })
    if (typeof selected === 'string') {
      restoreFile.value = selected
      restoreResult.value = null
    }
  } catch (err) {
    restoreResult.value = { ok: false, text: String(err) }
  }
}

async function runRestore(): Promise<void> {
  if (!restoreFile.value || restoring.value) return
  let ok = false
  try {
    ok = await ask(
      `Restore ${restoreFile.value} into ${restoreService.value === 'mysql' ? 'MySQL' : 'PostgreSQL'}? ` +
        'Statements in the dump run as-is (it may drop or overwrite tables).',
      { title: 'Restore database', kind: 'warning' },
    )
  } catch {
    ok = window.confirm('Restore this dump?')
  }
  if (!ok) return
  restoring.value = true
  restoreResult.value = null
  try {
    await invoke<void>('services_restore', { service: restoreService.value, file: restoreFile.value })
    restoreResult.value = { ok: true, text: 'Restore finished. Details are in the Logs.' }
  } catch (err) {
    restoreResult.value = { ok: false, text: String(err) }
  } finally {
    restoring.value = false
  }
}

// Migrasi Docker → WSL containers: hanya relevan bila engine aktif WSLC dan
// Docker masih terpasang (sumber data lama).
const canMigrateFromDocker = computed(
  () => engineInfo.value?.engine === 'wslc' && !!engineInfo.value?.docker_available,
)

async function runMigrateFromDocker(): Promise<void> {
  if (restoring.value) return
  const label = restoreService.value === 'mysql' ? 'MySQL' : 'PostgreSQL'
  let ok = false
  try {
    ok = await ask(
      `Copy ${label} databases from the Docker container into WSL containers? ` +
        'Docker must be running. The old Docker container is left untouched.',
      { title: 'Import from Docker', kind: 'info' },
    )
  } catch {
    ok = window.confirm(`Import ${label} from Docker?`)
  }
  if (!ok) return
  restoring.value = true
  restoreResult.value = null
  try {
    await invoke<void>('services_migrate_from_docker', { service: restoreService.value })
    restoreResult.value = { ok: true, text: `${label} imported from Docker. Details are in the Logs.` }
  } catch (err) {
    restoreResult.value = { ok: false, text: String(err) }
  } finally {
    restoring.value = false
  }
}

async function handleRemoveProject(id: string, name: string): Promise<void> {
  let ok = false
  try {
    ok = await ask(`Remove project "${name}" from Servel? Files and mise.toml are not touched.`, {
      title: 'Remove project',
      kind: 'warning',
    })
  } catch {
    ok = window.confirm(`Remove project "${name}"?`)
  }
  if (!ok) return
  await removeProject(id)
  await versionCtx.refresh()
}

async function fetchComposePath(): Promise<void> {
  try {
    composePath.value = await invoke<string>('get_compose_path')
  } catch {
    composePath.value = '—'
  }
}

async function checkEngine(): Promise<void> {
  checkingEngine.value = true
  try {
    engineInfo.value = await invoke<PrereqStatus>('check_prerequisites')
  } catch {
    engineInfo.value = null
  } finally {
    checkingEngine.value = false
  }
}

async function chooseEngine(value: EnginePreference): Promise<void> {
  if (switchingEngine.value || config.value.containerRuntime === value) return
  switchingEngine.value = true
  try {
    await setContainerRuntime(value)
    await checkEngine()
    await refreshEngineBanner()
  } finally {
    switchingEngine.value = false
  }
}

async function handleReset(): Promise<void> {
  let ok = false
  try {
    ok = await ask(
      'Reset all settings? This will restore defaults and clear saved selection.',
      { title: 'Reset Settings', kind: 'warning' },
    )
  } catch {
    ok = window.confirm('Reset all settings?')
  }
  if (!ok) return
  await reset()
  const { push } = useLogs('SERVEL')
  push({ ts: new Date().toTimeString().slice(0, 8), src: 'SERVEL', text: 'settings reset to defaults' })
}

onMounted(async () => {
  if (!loaded.value) await load()
  try {
    platform.value = await invoke<string>('get_platform')
  } catch {
    /* default windows */
  }
  await fetchComposePath()
  await checkEngine()
  void versionCtx.refresh()
  await refreshSitesStatus()
  await refreshProxy()
  try {
    version.value = await getVersion()
  } catch {
    version.value = ''
  }
})
</script>

<template>
  <div class="view view-settings">
    <header class="app-strip">
      <button class="app-strip__back" @click="setView('dashboard')">&#8592;</button>
      <span class="app-strip__title">SETTINGS</span>
      <span class="app-strip__spacer"></span>
    </header>

    <div class="settings-body">
      <nav class="settings-nav">
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'general' }"
          @click="activeNav = 'general'"
        >General</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'services' }"
          @click="activeNav = 'services'"
        >Services</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'sites' }"
          @click="activeNav = 'sites'"
        >Sites</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'php_node' }"
          @click="activeNav = 'php_node'"
        >PHP &amp; Node</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'about' }"
          @click="activeNav = 'about'"
        >About</button>
      </nav>

      <main class="settings-content">
        <template v-if="activeNav === 'general'">
          <div class="section-block">
            <div class="section-title">GENERAL</div>

            <SettingRow
              label="Auto-start infra on launch"
              desc="Start saved selection automatically when Servel boots"
            >
              <button
                class="toggle"
                :class="{ 'toggle--on': config.autoStart }"
                role="switch"
                :aria-checked="config.autoStart"
                @click="setAutoStart(!config.autoStart)"
              >
                <span class="toggle-knob"></span>
              </button>
            </SettingRow>

            <SettingRow
              label="Remember last session"
              desc="Persist selected services between launches"
            >
              <button
                class="toggle"
                :class="{ 'toggle--on': config.rememberSession }"
                role="switch"
                :aria-checked="config.rememberSession"
                @click="setRememberSession(!config.rememberSession)"
              >
                <span class="toggle-knob"></span>
              </button>
            </SettingRow>

            <SettingRow
              label="Minimize to tray on close"
              desc="Keep Servel running in system tray instead of quitting on window close"
            >
              <button
                class="toggle"
                :class="{ 'toggle--on': config.minimizeToTray }"
                role="switch"
                :aria-checked="config.minimizeToTray"
                @click="setMinimizeToTray(!config.minimizeToTray)"
              >
                <span class="toggle-knob"></span>
              </button>
            </SettingRow>
          </div>

          <div class="section-block">
            <div class="section-title">CONTAINER ENGINE</div>

            <SettingRow
              v-if="platform === 'windows'"
              label="Engine"
              desc="Auto uses WSL containers when Docker is not installed"
            >
              <div class="segmented" role="radiogroup" aria-label="Container engine">
                <button
                  v-for="opt in ENGINE_OPTIONS"
                  :key="opt.value"
                  class="segmented__item"
                  :class="{ 'segmented__item--on': config.containerRuntime === opt.value }"
                  role="radio"
                  :aria-checked="config.containerRuntime === opt.value"
                  :disabled="switchingEngine"
                  @click="chooseEngine(opt.value)"
                >{{ opt.label }}</button>
              </div>
            </SettingRow>

            <SettingRow
              label="Status"
              :desc="engineInfo?.engine_version ? `${activeEngineLabel} v${engineInfo.engine_version}` : activeEngineLabel"
            >
              <span
                class="status-pill"
                :class="engineRunning === true ? 'status-pill--green' : 'status-pill--red'"
              >
                <span class="status-dot"></span>
                <span v-if="engineRunning === true">Running</span>
                <span v-else-if="engineRunning === false">
                  {{ engineInfo?.engine_installed ? 'Stopped' : 'Not installed' }}
                </span>
                <span v-else>—</span>
              </span>
              <button
                class="action-btn"
                :disabled="checkingEngine"
                @click="checkEngine"
              >{{ checkingEngine ? '...' : 'Refresh' }}</button>
            </SettingRow>

            <SettingRow v-if="platform === 'windows'" label="WSL version" desc="Built-in containers need WSL 3.0.1 or newer">
              <span class="setting-value-mono">{{ engineInfo?.wsl_version ?? '—' }}</span>
            </SettingRow>

            <SettingRow
              v-if="platform === 'windows'"
              label="Terminal command"
              desc="Added to your PATH by the installer — servel up · down · status · logs · shell · restore"
            >
              <span class="setting-value-mono">servel help</span>
            </SettingRow>

            <SettingRow
              v-if="engineInfo?.engine !== 'wslc'"
              label="Docker Compose file path"
              desc="Path to generated compose file"
            >
              <span class="setting-value-mono">{{ composePath || '—' }}</span>
            </SettingRow>

            <p v-if="engineInfo?.engine === 'wslc'" class="section-hint section-hint--note">
              WSL containers keep their own images and volumes, separate from Docker Desktop.
              Databases created under Docker will not appear here until you restore them.
              <template v-if="engineInfo?.elevated">
                Servel is running as Administrator, which uses a separate WSL container session.
              </template>
            </p>
          </div>

          <div class="section-block section-block--danger">
            <div class="section-title section-title--red">DANGER ZONE</div>

            <SettingRow
              label="Reset all settings"
              desc="Restore defaults and clear saved selection"
            >
              <button class="action-btn action-btn--red" @click="handleReset">Reset</button>
            </SettingRow>
          </div>
        </template>

        <template v-else-if="activeNav === 'services'">
          <div class="section-block">
            <div class="section-title">PROFIL</div>
            <p class="section-hint">
              Preset pilihan service. Ganti profil aktif lewat switcher di Dashboard.
            </p>

            <div class="profile-list">
              <div v-for="p in config.profiles" :key="p.id" class="profile-item">
                <template v-if="editingId === p.id">
                  <input
                    v-model="editingName"
                    class="profile-edit-input"
                    type="text"
                    maxlength="40"
                    @keyup.enter="commitEditProfile"
                    @keyup.esc="cancelEditProfile"
                  />
                  <div class="profile-actions">
                    <button class="action-btn" @click="commitEditProfile">Simpan</button>
                    <button class="action-btn" @click="cancelEditProfile">Batal</button>
                  </div>
                </template>
                <template v-else>
                  <div class="profile-info">
                    <span class="profile-name">{{ p.name }}</span>
                    <span v-if="p.id === config.activeProfileId" class="profile-badge">aktif</span>
                    <span class="profile-count">{{ p.serviceIds.length }} service</span>
                  </div>
                  <div class="profile-actions">
                    <button class="action-btn" @click="startEditProfile(p.id, p.name)">Rename</button>
                    <button
                      class="action-btn action-btn--red"
                      :disabled="config.profiles.length <= 1"
                      :title="config.profiles.length <= 1 ? 'Minimal 1 profil' : 'Hapus profil'"
                      @click="handleDeleteProfile(p.id, p.name)"
                    >Hapus</button>
                  </div>
                </template>
              </div>
            </div>

            <div class="profile-create">
              <input
                v-model="newProfileName"
                class="profile-create-input"
                type="text"
                maxlength="40"
                placeholder="Nama profil baru…"
                @keyup.enter="handleCreateProfile"
              />
              <button
                class="action-btn action-btn--accent"
                :disabled="!newProfileName.trim()"
                @click="handleCreateProfile"
              >Buat dari selection</button>
            </div>
          </div>

          <div class="section-block">
            <div class="section-title">RESTORE DATABASE</div>
            <p class="section-hint">
              Import a .sql dump into MySQL or PostgreSQL. Servel starts the service, waits until it
              accepts connections, then runs the dump. Useful after switching engines.
            </p>

            <SettingRow label="Service">
              <div class="segmented" role="radiogroup" aria-label="Restore target">
                <button
                  v-for="opt in [{ v: 'mysql', l: 'MySQL' }, { v: 'postgres', l: 'PostgreSQL' }]"
                  :key="opt.v"
                  class="segmented__item"
                  :class="{ 'segmented__item--on': restoreService === opt.v }"
                  role="radio"
                  :aria-checked="restoreService === opt.v"
                  :disabled="restoring"
                  @click="restoreService = opt.v as 'mysql' | 'postgres'"
                >{{ opt.l }}</button>
              </div>
            </SettingRow>

            <SettingRow label="Dump file" :desc="restoreFile ?? 'No file chosen'">
              <button class="action-btn" :disabled="restoring" @click="pickRestoreFile">Choose…</button>
              <button
                class="action-btn"
                :disabled="!restoreFile || restoring"
                @click="runRestore"
              >{{ restoring ? 'Restoring…' : 'Restore' }}</button>
            </SettingRow>

            <SettingRow
              v-if="canMigrateFromDocker"
              label="Import from Docker"
              desc="Copy databases from your old Docker container (Docker Desktop must be running)"
            >
              <button class="action-btn" :disabled="restoring" @click="runMigrateFromDocker">Import…</button>
            </SettingRow>

            <p
              v-if="restoreResult"
              class="section-hint section-hint--note"
              :class="{ 'restore-error': !restoreResult.ok }"
            >{{ restoreResult.text }}</p>
          </div>
        </template>

        <template v-else-if="activeNav === 'sites'">
          <div class="section-block">
            <div class="section-title">SITES</div>
            <p class="section-hint">
              Domain lokal → IP lewat file hosts. Perubahan ditulis ke hosts hanya saat
              kamu klik <strong>Terapkan ke hosts</strong> (butuh izin admin/UAC).
            </p>

            <div class="sites-statusbar">
              <span
                v-if="sitesStatus"
                class="status-pill"
                :class="sitesStatus.inSync ? 'status-pill--green' : 'status-pill--amber'"
              >
                <span class="status-dot"></span>
                {{ sitesStatus.inSync ? 'Sinkron dengan hosts' : 'Ada perubahan belum diterapkan' }}
              </span>
              <button
                class="action-btn action-btn--accent"
                :disabled="sitesBusy || (sitesStatus?.inSync ?? false)"
                @click="openApply"
              >{{ sitesBusy ? 'Menerapkan…' : 'Terapkan ke hosts' }}</button>
            </div>

            <div class="site-list">
              <div v-for="s in sites" :key="s.id" class="site-item">
                <template v-if="editingSiteId === s.id">
                  <input
                    v-model="editDomain"
                    class="profile-edit-input"
                    type="text"
                    maxlength="253"
                    placeholder="domain.test"
                    @keyup.enter="commitEditSite"
                    @keyup.esc="cancelEditSite"
                  />
                  <input
                    v-model="editIp"
                    class="profile-edit-input site-ip-input"
                    type="text"
                    maxlength="15"
                    placeholder="127.0.0.1"
                    @keyup.enter="commitEditSite"
                    @keyup.esc="cancelEditSite"
                  />
                  <input
                    v-model="editPort"
                    class="profile-edit-input site-port-input"
                    type="text"
                    maxlength="5"
                    placeholder="port"
                    title="Port dev server tujuan. Kosongkan bila domain ini hanya perlu resolve nama."
                    @keyup.enter="commitEditSite"
                    @keyup.esc="cancelEditSite"
                  />
                  <div class="profile-actions">
                    <button class="action-btn" @click="commitEditSite">Simpan</button>
                    <button class="action-btn" @click="cancelEditSite">Batal</button>
                  </div>
                </template>
                <template v-else>
                  <div class="site-info">
                    <button
                      class="toggle toggle--sm"
                      :class="{ 'toggle--on': s.enabled }"
                      role="switch"
                      :aria-checked="s.enabled"
                      :title="s.enabled ? 'Nonaktifkan' : 'Aktifkan'"
                      @click="handleToggleSite(s.id)"
                    >
                      <span class="toggle-knob"></span>
                    </button>
                    <span class="site-domain" :class="{ 'site-domain--off': !s.enabled }">{{ s.domain }}</span>
                    <span class="site-arrow">→</span>
                    <span class="site-ip">{{ s.ip }}</span>
                    <span v-if="s.target" class="site-port">:{{ s.target.value }}</span>
                    <span v-else class="site-port site-port--none" title="Nama resolve, tapi tak ada yang mem-proxy-kan">
                      tanpa proxy
                    </span>
                  </div>
                  <div class="profile-actions">
                    <button
                      class="action-btn"
                      @click="startEditSite(s.id, s.domain, s.ip, s.target?.value ?? null)"
                    >Edit</button>
                    <button
                      class="action-btn action-btn--red"
                      @click="handleDeleteSite(s.id, s.domain)"
                    >Hapus</button>
                  </div>
                </template>
              </div>
              <p v-if="sites.length === 0" class="placeholder-text">
                Belum ada domain. Tambahkan di bawah.
              </p>
            </div>

            <div class="site-create">
              <input
                v-model="newDomain"
                class="profile-create-input"
                type="text"
                maxlength="253"
                placeholder="myapp.test"
                @keyup.enter="handleAddSite"
              />
              <input
                v-model="newIp"
                class="profile-create-input site-ip-input"
                type="text"
                maxlength="15"
                placeholder="127.0.0.1"
                @keyup.enter="handleAddSite"
              />
              <input
                v-model="newPort"
                class="profile-create-input site-port-input"
                type="text"
                maxlength="5"
                placeholder="port"
                title="Port dev server tujuan, mis. 5173. Kosongkan bila domain ini hanya perlu resolve nama."
                @keyup.enter="handleAddSite"
              />
              <button
                class="action-btn action-btn--accent"
                :disabled="!newDomain.trim()"
                @click="handleAddSite"
              >Tambah</button>
            </div>
            <p v-if="siteError" class="site-error">{{ siteError }}</p>
            <p v-if="applyError && !showDiffModal" class="site-error">{{ applyError }}</p>
          </div>

          <div class="section-block">
            <div class="section-title">REVERSE PROXY</div>
            <p class="section-hint">
              Hosts hanya membuat nama domain resolve ke IP. Proxy yang meneruskannya ke dev server
              kamu — jalankan dev server sendiri, isi port tujuannya di daftar domain di atas.
            </p>

            <div v-if="!proxyBinary?.installed" class="sites-statusbar">
              <span class="status-pill status-pill--amber">
                <span class="status-dot"></span>
                Engine proxy belum terpasang
              </span>
              <button
                class="action-btn action-btn--accent"
                :disabled="proxyBusy"
                @click="handleInstallProxyBinary"
              >
                {{
                  proxyBusy && proxyProgress
                    ? `Mengunduh… ${Math.round((proxyProgress.downloaded / (proxyProgress.total || 1)) * 100)}%`
                    : proxyBusy
                      ? 'Mengunduh…'
                      : 'Pasang engine proxy'
                }}
              </button>
            </div>

            <template v-else>
              <div class="sites-statusbar">
                <span
                  class="status-pill"
                  :class="proxyStatus?.running ? 'status-pill--green' : 'status-pill--amber'"
                >
                  <span class="status-dot"></span>
                  {{
                    proxyStatus?.running
                      ? `Berjalan — ${proxyStatus.routed_sites} domain${proxyStatus.https ? ' (HTTPS)' : ' (HTTP)'}`
                      : 'Proxy berhenti'
                  }}
                </span>
                <button
                  class="action-btn action-btn--accent"
                  :disabled="proxyBusy || (!proxyStatus?.running && proxyableCount === 0)"
                  :title="
                    !proxyStatus?.running && proxyableCount === 0
                      ? 'Aktifkan minimal satu domain dan isi port tujuannya'
                      : ''
                  "
                  @click="handleToggleProxy"
                >
                  {{ proxyBusy ? 'Memproses…' : proxyStatus?.running ? 'Hentikan' : 'Jalankan' }}
                </button>
              </div>

              <div class="sites-statusbar">
                <span
                  class="status-pill"
                  :class="proxyStatus?.cert_installed ? 'status-pill--green' : 'status-pill--amber'"
                >
                  <span class="status-dot"></span>
                  {{
                    proxyStatus?.cert_installed
                      ? 'Sertifikat lokal terpasang'
                      : 'Belum ada sertifikat — domain dilayani lewat HTTP'
                  }}
                </span>
                <button
                  v-if="!proxyStatus?.cert_installed"
                  class="action-btn"
                  :disabled="proxyBusy"
                  @click="handleInstallCert"
                >Pasang sertifikat</button>
              </div>
              <p class="section-hint">
                Sertifikat lokal membuat <code>https://</code> dipercaya browser. Windows akan
                menampilkan dialog konfirmasi; menolaknya tidak masalah — domain tetap bisa dipakai
                lewat HTTP.
              </p>
            </template>

            <p v-if="proxyError" class="site-error">{{ proxyError }}</p>
          </div>

          <div v-if="(sitesStatus?.backups.length ?? 0) > 0" class="section-block">
            <div class="section-title">RESTORE BACKUP</div>
            <p class="section-hint">Pulihkan file hosts dari backup otomatis (5 terakhir).</p>
            <div class="site-create">
              <select v-model="selectedBackup" class="profile-create-input">
                <option value="">Pilih backup…</option>
                <option v-for="b in sitesStatus?.backups" :key="b" :value="b">{{ b }}</option>
              </select>
              <button
                class="action-btn action-btn--red"
                :disabled="!selectedBackup || sitesBusy"
                @click="handleRestore"
              >Restore</button>
            </div>
          </div>
        </template>

        <template v-else-if="activeNav === 'php_node'">
          <div class="section-block">
            <div class="section-title">VERSION MANAGER</div>

            <SettingRow
              label="Manage PHP & Node with"
              desc="Auto uses mise when it is installed, otherwise phpvm and fnm"
            >
              <div class="segmented" role="radiogroup" aria-label="Version manager">
                <button
                  v-for="opt in PROVIDER_OPTIONS"
                  :key="opt.value"
                  class="segmented__item"
                  :class="{ 'segmented__item--on': config.versionProvider === opt.value }"
                  role="radio"
                  :aria-checked="config.versionProvider === opt.value"
                  @click="chooseProvider(opt.value)"
                >{{ opt.label }}</button>
              </div>
            </SettingRow>

            <SettingRow
              label="mise"
              :desc="miseCtx?.provider === 'mise' ? 'Active — versions resolve per project folder' : 'Not in use'"
            >
              <span class="setting-value-mono">{{ miseCtx?.miseVersion ? `v${miseCtx.miseVersion}` : 'not installed' }}</span>
            </SettingRow>

            <SettingRow
              v-if="miseCtx?.miseVersion"
              label="Repair"
              desc="Reshim fixes missing commands; Clear cache refreshes version lists; Doctor writes a report to the log"
            >
              <button class="action-btn" :disabled="miseBusy" @click="versionCtx.repair('reshim')">Reshim</button>
              <button class="action-btn" :disabled="miseBusy" @click="versionCtx.repair('cache-clear')">Clear cache</button>
              <button class="action-btn" :disabled="miseBusy" @click="versionCtx.repair('doctor')">Doctor</button>
            </SettingRow>

            <p v-if="miseError" class="section-hint section-hint--note">{{ miseError }}</p>
          </div>

          <div class="section-block">
            <div class="section-title">PROJECTS</div>
            <p class="section-hint">
              Folders whose PHP/Node versions you want to see and switch from the Dashboard.
              mise resolves each folder on its own, including a mise.toml in a parent folder.
              Link a service profile to a project to switch its services along with it.
            </p>

            <div class="profile-list">
              <div v-for="p in config.projects" :key="p.id" class="profile-item">
                <div class="profile-info">
                  <span class="profile-name">{{ p.name }}</span>
                  <span v-if="p.id === config.activeProjectId" class="profile-badge">active</span>
                  <span class="site-domain project-path" :title="p.path">{{ p.path }}</span>
                </div>
                <div class="profile-actions">
                  <select
                    class="profile-create-input project-profile-select"
                    :value="p.profileId ?? ''"
                    title="Service profile applied when this project is selected"
                    @change="setProjectProfile(p.id, ($event.target as HTMLSelectElement).value || null)"
                  >
                    <option value="">No service profile</option>
                    <option v-for="pr in config.profiles" :key="pr.id" :value="pr.id">Profile: {{ pr.name }}</option>
                  </select>
                  <button class="action-btn action-btn--red" @click="handleRemoveProject(p.id, p.name)">Remove</button>
                </div>
              </div>
              <p v-if="config.projects.length === 0" class="section-hint">No projects yet.</p>
            </div>

            <div class="profile-create">
              <button class="action-btn" @click="pickProject">+ Add project folder</button>
            </div>
          </div>
        </template>

        <template v-else-if="activeNav === 'about'">
          <div class="section-block">
            <div class="section-title">ABOUT</div>
            <div class="about-block">
              <span class="about-name">servel</span>
              <span class="about-desc">Local dev environment manager</span>
              <span class="about-version">v{{ version || '...' }}</span>
            </div>

            <div v-if="updateAvailable" class="update-banner">
              <span class="ub-text">Versi baru tersedia: <strong>v{{ latestVersion }}</strong></span>
              <button class="ub-download" @click="openReleasePage">Buka halaman rilis</button>
            </div>

            <div class="update-row">
              <button class="update-check-btn" :disabled="checkingUpdate" @click="checkUpdate">
                {{ checkingUpdate ? 'Memeriksa…' : 'Cek update' }}
              </button>
              <span v-if="!updateAvailable && lastChecked" class="update-uptodate">Sudah versi terbaru</span>
            </div>
          </div>
        </template>
      </main>
    </div>

    <div v-if="showDiffModal" class="modal-overlay" @click.self="cancelApply">
      <div class="modal-card">
        <div class="modal-title">Konfirmasi perubahan hosts</div>
        <p class="modal-desc">
          Servel akan menulis blok terkelola di file hosts. Baris di luar blok tidak disentuh.
          Klik <strong>Ya, terapkan</strong> lalu setujui prompt UAC.
        </p>

        <div class="diff-box">
          <div v-for="l in pendingDiff?.removed ?? []" :key="'r-' + l" class="diff-line diff-line--del">- {{ l }}</div>
          <div v-for="l in pendingDiff?.added ?? []" :key="'a-' + l" class="diff-line diff-line--add">+ {{ l }}</div>
          <div v-for="l in pendingDiff?.unchanged ?? []" :key="'u-' + l" class="diff-line diff-line--same">&nbsp;&nbsp;{{ l }}</div>
          <div
            v-if="(pendingDiff?.added.length ?? 0) === 0 && (pendingDiff?.removed.length ?? 0) === 0"
            class="diff-line diff-line--same"
          >Tidak ada perubahan entri.</div>
        </div>

        <p v-if="applyError" class="site-error">{{ applyError }}</p>

        <div class="modal-actions">
          <button class="action-btn" :disabled="sitesBusy" @click="cancelApply">Batal</button>
          <button
            class="action-btn action-btn--accent"
            :disabled="sitesBusy"
            @click="confirmApply"
          >{{ sitesBusy ? 'Menerapkan…' : 'Ya, terapkan' }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.view-settings {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
}

.app-strip {
  display: flex;
  align-items: center;
  padding: 0 var(--space-4);
  height: 36px;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  background: var(--surface);
  flex-shrink: 0;
  gap: var(--space-4);
}

.app-strip__back {
  background: transparent;
  border: none;
  color: var(--muted);
  font-size: 16px;
  cursor: pointer;
  padding: 0 var(--space-2);
  line-height: 1;
  border-radius: var(--radius-xs);
  transition: color 0.1s, background 0.1s;
}

.app-strip__back:hover {
  color: var(--text);
  background: var(--surface2);
}

.app-strip__title {
  font-family: var(--font-sans);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: var(--text);
  flex: 1;
  text-align: center;
}

.app-strip__spacer {
  width: 32px;
}

.settings-body {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.settings-nav {
  width: 180px;
  flex-shrink: 0;
  border-right: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  background: var(--surface);
  padding: var(--space-4) 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.snav-item {
  display: block;
  width: 100%;
  text-align: left;
  background: transparent;
  border: none;
  border-left: 2px solid transparent;
  padding: var(--space-2) var(--space-4);
  font-family: var(--font-sans);
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--muted);
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s, background 0.1s;
}

.snav-item:hover:not(.snav-item--active) {
  color: var(--text);
  background: color-mix(in srgb, var(--surface2) 50%, transparent);
}

.snav-item--active {
  color: var(--accent);
  border-left-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}

.settings-content {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.section-block {
  display: flex;
  flex-direction: column;
}

.section-block--danger {
  margin-top: var(--space-2);
}

.section-title {
  font-family: var(--font-sans);
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.07em;
  color: var(--dim);
  margin-bottom: var(--space-3);
}

.section-title--red {
  color: color-mix(in srgb, var(--red) 80%, transparent);
}

/* Toggle */
.toggle {
  position: relative;
  width: 36px;
  height: 20px;
  border-radius: var(--radius-md);
  background: var(--surface2);
  border: 1px solid var(--border);
  cursor: pointer;
  padding: 0;
  transition: background 0.15s, border-color 0.15s;
}

.toggle--on {
  background: color-mix(in srgb, var(--accent) 35%, transparent);
  border-color: color-mix(in srgb, var(--accent) 60%, transparent);
}

.toggle-knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--muted);
  transition: transform 0.15s, background 0.15s;
}

.toggle--on .toggle-knob {
  transform: translateX(16px);
  background: var(--accent);
}

/* Status pill */
.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 10px;
  border-radius: 20px;
  font-family: var(--font-sans);
  font-size: 11px;
  border: 1px solid;
}

.status-pill--green {
  background: color-mix(in srgb, var(--green) 12%, transparent);
  border-color: color-mix(in srgb, var(--green) 35%, transparent);
  color: var(--green);
}

.status-pill--red {
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border-color: color-mix(in srgb, var(--red) 35%, transparent);
  color: var(--red);
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

/* Mono value display */
.setting-value-mono {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Action buttons */
.action-btn {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 12px;
  border-radius: var(--radius-xs);
  cursor: pointer;
  background: var(--surface2);
  border: 1px solid var(--border);
  color: var(--muted);
  transition: background 0.1s, color 0.1s;
}

.action-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--surface2) 80%, var(--text) 20%);
  color: var(--text);
}

.action-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.action-btn--red {
  background: color-mix(in srgb, var(--red) 10%, transparent);
  border-color: color-mix(in srgb, var(--red) 40%, transparent);
  color: var(--red);
}

.action-btn--red:hover:not(:disabled) {
  background: color-mix(in srgb, var(--red) 20%, transparent);
  border-color: color-mix(in srgb, var(--red) 65%, transparent);
}

/* About section */
.about-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-4);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
}

.about-name {
  font-family: var(--font-mono);
  font-size: 18px;
  font-weight: 700;
  color: var(--accent);
}

.about-desc {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--muted);
}

.about-version {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
}

.update-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  margin-top: var(--space-3);
  padding: var(--space-3) var(--space-4);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
  border-radius: var(--radius-sm);
}

.ub-text {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--text);
}

.ub-download {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 12px;
  background: color-mix(in srgb, var(--accent) 18%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  border-radius: var(--radius-xs);
  color: var(--accent);
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.1s, border-color 0.1s;
}

.ub-download:hover {
  background: color-mix(in srgb, var(--accent) 28%, transparent);
  border-color: color-mix(in srgb, var(--accent) 65%, transparent);
}

.update-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-top: var(--space-3);
}

.update-check-btn {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-xs);
  color: var(--text);
  cursor: pointer;
  transition: border-color 0.1s;
}

.update-check-btn:hover:not(:disabled) {
  border-color: var(--dim);
}

.update-check-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.update-uptodate {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--muted);
}

/* Placeholder */
.placeholder-text {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--dim);
  margin: 0;
  padding: var(--space-4) 0;
}

/* Profiles CRUD */
.section-hint {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--dim);
  margin: 0 0 var(--space-3);
}

.section-hint--note {
  margin: var(--space-3) 0 0;
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  background: color-mix(in srgb, var(--accent) 7%, var(--surface));
  color: var(--muted);
  line-height: 1.6;
}

.segmented {
  display: inline-flex;
  padding: 3px;
  gap: 2px;
  border-radius: var(--radius-md);
  background: var(--surface2);
}

.segmented__item {
  font-family: var(--font-sans);
  font-size: 12px;
  padding: 5px 12px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.segmented__item:hover:not(:disabled) {
  color: var(--text);
}

.segmented__item--on {
  background: var(--surface);
  color: var(--text);
  box-shadow: var(--shadow-sm);
}

.segmented__item:disabled {
  cursor: progress;
}

.profile-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.profile-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}

.profile-info {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.profile-name {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-badge {
  font-family: var(--font-sans);
  font-size: 9px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  padding: 1px 6px;
  border-radius: var(--radius-md);
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
  color: var(--accent);
  flex-shrink: 0;
}

.profile-count {
  font-family: var(--font-sans);
  font-size: 10px;
  color: var(--dim);
  flex-shrink: 0;
}

.profile-actions {
  display: flex;
  gap: var(--space-2);
  flex-shrink: 0;
}

.profile-edit-input,
.profile-create-input {
  flex: 1;
  min-width: 0;
  font-family: var(--font-sans);
  font-size: 12px;
  padding: 5px 8px;
  border-radius: var(--radius-xs);
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--text);
}

.profile-edit-input:focus,
.profile-create-input:focus {
  outline: none;
  border-color: color-mix(in srgb, var(--accent) 55%, transparent);
}

.profile-create {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-top: var(--space-4);
}

.action-btn--accent {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  color: var(--accent);
  flex-shrink: 0;
}

.action-btn--accent:hover:not(:disabled) {
  background: color-mix(in srgb, var(--accent) 22%, transparent);
  border-color: color-mix(in srgb, var(--accent) 65%, transparent);
}

/* Sites (hosts) */
.status-pill--amber {
  background: color-mix(in srgb, var(--amber) 12%, transparent);
  border-color: color-mix(in srgb, var(--amber) 35%, transparent);
  color: var(--amber);
}

.sites-statusbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  margin-bottom: var(--space-4);
}

.site-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.site-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}

.site-info {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.toggle--sm {
  width: 30px;
  height: 17px;
  flex-shrink: 0;
}

.toggle--sm .toggle-knob {
  width: 11px;
  height: 11px;
}

.toggle--sm.toggle--on .toggle-knob {
  transform: translateX(13px);
}

.site-domain {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.site-domain--off {
  color: var(--dim);
  text-decoration: line-through;
}

.site-arrow {
  color: var(--dim);
  font-size: 11px;
  flex-shrink: 0;
}

.site-ip {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
  flex-shrink: 0;
}

.site-ip-input {
  flex: 0 0 110px;
  max-width: 110px;
}

.site-port-input {
  flex: 0 0 70px;
  max-width: 70px;
}

.site-port {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--muted);
}

.site-port--none {
  font-family: inherit;
  font-style: italic;
  opacity: 0.7;
}

.site-create {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-top: var(--space-4);
}

.site-error {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--red);
  margin: var(--space-2) 0 0;
}

/* Diff-confirm modal */
.modal-overlay {
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--bg) 70%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-6);
  z-index: 50;
}

.modal-card {
  width: 100%;
  max-width: 440px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-6);
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.modal-title {
  font-family: var(--font-sans);
  font-size: 13px;
  letter-spacing: 0.04em;
  color: var(--text);
}

.modal-desc {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--dim);
  margin: 0;
}

.diff-box {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-3);
  max-height: 200px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.diff-line {
  white-space: pre-wrap;
  word-break: break-all;
}

.diff-line--add {
  color: var(--green);
}

.diff-line--del {
  color: var(--red);
}

.diff-line--same {
  color: var(--muted);
}

.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

.project-path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--dim);
}

.restore-error {
  color: var(--red);
  background: color-mix(in srgb, var(--red) 8%, var(--surface));
}

.project-profile-select {
  width: auto;
  max-width: 220px;
}
</style>
