<script setup lang="ts">
import { openExternal } from '@/utils/openExternal'
import UiChip from '@/components/ui/UiChip.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import { useToast } from '@/composables/useToast'
import { ref, computed, inject, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { appDataDir } from '@tauri-apps/api/path'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { useVersionContext } from '@/composables/useVersionContext'
import { useRuntimeLocation } from '@/composables/useRuntimeLocation'
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import { useUpdateCheck } from '@/composables/useUpdateCheck'
import RuntimeTools from '@/components/RuntimeTools.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import SettingRow from '@/components/SettingRow.vue'
import UiModal from '@/components/ui/UiModal.vue'
import { useServices } from '@/composables/useServices'
import UiStatusNotice from '@/components/ui/UiStatusNotice.vue'
import { ENGINE_LABEL, type EnginePreference, type PrereqStatus } from '@/types/prereq'
import { useEngineStatus } from '@/composables/useEngineStatus'

type SettingsNav = 'general' | 'services' | 'sites' | 'php_node' | 'about'

const setView = inject(SetViewKey)!

const {
  config,
  loaded,
  load,
  setAutoStart,
  setAutoStartProjects,
  setRememberSession,
  setMinimizeToTray,
  setUiScale,
  setStopServicesOnQuit,
  setContainerRuntime,
  createProfile,
  renameProfile,
  deleteProfile,
  reset,
} = useConfig()

/** Which projects the "start on launch" switch covers, as on the canvas. */
const launchProjectsDesc = computed(() => {
  const names = config.value.projects.filter((p) => p.autoStart).map((p) => p.name)
  if (names.length === 0) return 'No project is marked yet. Turn it on per project in ⋯ → Project settings.'
  return `${names.length} ${names.length === 1 ? 'project' : 'projects'}: ${names.join(', ')}`
})

// Settings tampil sebagai dua halaman di navigasi kiri: "settings"
// (General, Services, About) dan "runtimes" (PHP/Node, runtime lain, extension).
const props = withDefaults(defineProps<{ mode?: 'settings' | 'runtimes' }>(), { mode: 'settings' })
const activeNav = ref<SettingsNav>(props.mode === 'runtimes' ? 'php_node' : 'general')
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
    ok = await ask(`Delete profile "${name}"?`, { title: 'Delete profile', kind: 'warning' })
  } catch {
    ok = window.confirm(`Delete profile "${name}"?`)
  }
  if (!ok) return
  deleteProfile(id)
}

const {
  latestVersion,
  updateAvailable,
  checking: checkingUpdate,
  lastChecked,
  checkError: updateCheckError,
  canInstall,
  phase: updatePhase,
  bannerVisible: updateBannerVisible,
  installOnQuit,
  check: checkUpdate,
  download: downloadUpdate,
  apply: applyUpdate,
  openReleasePage,
} = useUpdateCheck()

const updateCheckedText = computed(() =>
  lastChecked.value ? new Date(lastChecked.value).toTimeString().slice(0, 5) : '',
)

const composePath = ref<string>('')
const engineInfo = ref<PrereqStatus | null>(null)
const checkingEngine = ref(false)
const switchingEngine = ref(false)
const platform = ref<string>('windows')
const version = ref<string>('')

// Diagnostics di About.
const dataDir = ref('')
const toast = useToast()
const { engineLabel: diagEngine, engineStatus: diagEngineStatus } = useEngineStatus()
async function loadDataDir(): Promise<void> {
  try {
    dataDir.value = await appDataDir()
  } catch {
    dataDir.value = ''
  }
}
async function copyDiagnostics(): Promise<void> {
  const platform = (await invoke<string>('get_platform').catch(() => '')) || navigator.platform
  const ctx = versionCtx.ctx.value
  const text = [
    `Kova ${version.value}`,
    `OS: ${platform} (${navigator.userAgent})`,
    `Engine: ${diagEngine.value} · ${diagEngineStatus.value}`,
    `mise: ${ctx?.miseVersion ?? 'not installed'}`,
    `PHP (global): ${ctx?.php?.version ?? '-'} · Node (global): ${ctx?.node?.version ?? '-'}`,
    `Data folder: ${dataDir.value}`,
    `Projects: ${config.value.projects.length} · Profiles: ${config.value.profiles.length}`,
  ].join('\n')
  try {
    await navigator.clipboard.writeText(text)
    toast.show('Copied diagnostics. Paste it into the bug report.')
  } catch {
    toast.error('Could not copy the diagnostics.', { copy: false })
  }
}
async function openDataDir(): Promise<void> {
  try {
    await invoke('project_open_in', { target: 'folder', path: dataDir.value })
  } catch (err) {
    toast.error(String(err))
  }
}

const engineRunning = computed<boolean | null>(() => engineInfo.value?.engine_running ?? null)
const activeEngineLabel = computed<string>(() =>
  engineInfo.value ? ENGINE_LABEL[engineInfo.value.engine] : 'Checking…',
)

const ENGINE_OPTIONS: { value: EnginePreference; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  { value: 'wslc', label: 'WSL containers' },
  { value: 'docker', label: 'Docker' },
]

const { refresh: refreshEngineBanner } = useEngineStatus()

// Version manager (mise) & projects
const versionCtx = useVersionContext()
const { ctx: miseCtx, busy: miseBusy, lastError: miseError } = versionCtx
const runtimeLoc = useRuntimeLocation()
const wslMiseVersion = computed(() => runtimeLoc.distroMise.value)

// State "mise is not installed" di Runtimes.
const isWindowsHost = navigator.userAgent.includes('Windows')
const installingMise = ref(false)
async function installMise(): Promise<void> {
  installingMise.value = true
  try {
    await invoke<string>('mise_install_winget')
    await versionCtx.refresh()
  } catch (err) {
    versionCtx.lastError.value = String(err)
  } finally {
    installingMise.value = false
  }
}
function openMiseDocs(): void {
  void openExternal('https://mise.jdx.dev/getting-started.html')
}
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

async function fetchComposePath(): Promise<void> {
  try {
    composePath.value = await invoke<string>('get_compose_path')
  } catch {
    composePath.value = ''
  }
}

// Waktu pemeriksaan terakhir, ditampilkan di samping Check again.
const engineCheckedAt = ref<number | null>(null)
const engineCheckedText = computed(() => {
  if (engineCheckedAt.value === null) return ''
  const t = new Date(engineCheckedAt.value)
  return `Checked at ${t.toTimeString().slice(0, 5)}`
})

async function checkEngine(): Promise<void> {
  checkingEngine.value = true
  try {
    engineInfo.value = await invoke<PrereqStatus>('check_prerequisites')
  } catch {
    engineInfo.value = null
  } finally {
    checkingEngine.value = false
    engineCheckedAt.value = Date.now()
  }
}

// Docker dipakai, tetapi WSL containers sudah ada di PC ini.
const suggestWslc = computed(
  () =>
    platform.value === 'windows' &&
    engineInfo.value?.engine === 'docker' &&
    !!engineInfo.value?.wslc_available,
)

// Kembali ke Kova setelah memasang WSL/Docker di luar Kova memeriksa
// ulang engine, supaya "not installed" tidak tertahan sampai Kova ditutup.
let lastEngineCheck = 0
function onWindowFocus(): void {
  if (checkingEngine.value || Date.now() - lastEngineCheck < 10_000) return
  lastEngineCheck = Date.now()
  void checkEngine()
}
onMounted(() => {
  lastEngineCheck = Date.now()
  window.addEventListener('focus', onWindowFocus)
})
onUnmounted(() => window.removeEventListener('focus', onWindowFocus))

/** Engine waiting for confirmation in the Switch engine dialog. */
const pendingEngine = ref<EnginePreference | null>(null)
const { runningCount: servicesRunning } = useServices()
function requestEngine(value: EnginePreference): void {
  if (switchingEngine.value || config.value.containerRuntime === value) return
  pendingEngine.value = value
}
async function confirmEngine(): Promise<void> {
  const v = pendingEngine.value
  pendingEngine.value = null
  if (v) await chooseEngine(v)
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

const resetMessage = computed(() => {
  const n = config.value.projects?.length ?? 0
  const what = n ? `all ${n} project${n === 1 ? '' : 's'}, their environments and groups` : 'your projects'
  return `This removes ${what}, your service profiles and custom services from Kova, then opens setup again. Project folders and database volumes stay on disk.`
})

async function handleReset(): Promise<void> {
  let ok = false
  try {
    ok = await ask(resetMessage.value, { title: 'Reset Kova?', kind: 'warning', okLabel: 'Remove everything and reset' })
  } catch {
    ok = window.confirm(resetMessage.value)
  }
  if (!ok) return
  await reset()
  const { push } = useLogs('KOVA')
  push({ ts: new Date().toTimeString().slice(0, 8), src: 'KOVA', text: 'settings reset to defaults' })
}

// "Start Kova when you sign in": disimpan oleh OS, bukan di config.
const autostartOn = ref(false)
const autostartBusy = ref(false)
const autostartError = ref('')
async function toggleAutostart(): Promise<void> {
  autostartBusy.value = true
  autostartError.value = ''
  try {
    autostartOn.value = await invoke<boolean>('autostart_set', { enabled: !autostartOn.value })
  } catch (err) {
    autostartError.value = String(err)
  } finally {
    autostartBusy.value = false
  }
}

// Perintah terminal `kova` (macOS/Linux): disalin ke ~/.local/bin atas permintaan.
interface CliStatus { supported: boolean; installed: boolean; outdated: boolean; path: string; onPath: boolean }
const cli = ref<CliStatus | null>(null)
const cliBusy = ref(false)
const cliError = ref('')
const cliDesc = computed(() => {
  const c = cli.value
  if (!c) return ''
  if (!c.installed) return `Copies the kova command to ${c.path}`
  if (c.outdated) return `Installed at ${c.path}, from an older Kova version`
  return `Installed at ${c.path}`
})
async function installCli(): Promise<void> {
  cliBusy.value = true
  cliError.value = ''
  try {
    cli.value = await invoke<CliStatus>('cli_install')
    useToast().show(`Installed the kova command at ${cli.value.path}`)
  } catch (err) {
    cliError.value = String(err)
  } finally {
    cliBusy.value = false
  }
}

// Windows: CLI dipasang installer; Reinstall menambah lagi folder-nya ke PATH user.
interface WinCliStatus { binDir: string; filesPresent: boolean; onPath: boolean }
const winCli = ref<WinCliStatus | null>(null)
const winCliBusy = ref(false)
const winCliError = ref('')
const winCliDesc = computed(() => {
  const c = winCli.value
  if (!c) return 'Checking…'
  if (!c.filesPresent) return `The kova files are missing from ${c.binDir}. Run the Kova installer again.`
  if (!c.onPath) return `Not on your PATH. Add ${c.binDir} so new terminals find kova.`
  return `On your PATH from ${c.binDir}. Open a new terminal after a reinstall.`
})
async function reinstallWinCli(): Promise<void> {
  winCliBusy.value = true
  winCliError.value = ''
  try {
    winCli.value = await invoke<WinCliStatus>('cli_windows_reinstall')
    useToast().show('Added kova to your PATH. Open a new terminal to use it.')
  } catch (err) {
    winCliError.value = String(err)
  } finally {
    winCliBusy.value = false
  }
}

onMounted(async () => {
  invoke<CliStatus>('cli_status').then((v) => (cli.value = v)).catch(() => {})
  invoke<boolean>('autostart_get').then((v) => (autostartOn.value = v)).catch(() => {})
  if (!loaded.value) await load()
  try {
    platform.value = await invoke<string>('get_platform')
    if (platform.value === 'windows') {
      invoke<WinCliStatus>('cli_windows_status').then((v) => (winCli.value = v)).catch(() => {})
    }
  } catch {
    /* default windows */
  }
  await fetchComposePath()
  await checkEngine()
  void versionCtx.refresh()
  try {
    version.value = await getVersion()
    void loadDataDir()
  } catch {
    version.value = ''
  }
})
</script>

<template>
  <div class="view view-settings">
    <!-- Judul layar seperti layar lain. -->
    <header v-if="props.mode === 'settings'" class="settings-head"><h1 class="settings-h1">Settings</h1></header>
    <div class="settings-body">
      <nav v-if="props.mode === 'settings'" class="settings-nav">
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'general' }"
          :aria-current="activeNav === 'general' ? 'page' : undefined"
          @click="activeNav = 'general'"
        >General</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'services' }"
          :aria-current="activeNav === 'services' ? 'page' : undefined"
          @click="activeNav = 'services'"
        >Services</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'about' }"
          :aria-current="activeNav === 'about' ? 'page' : undefined"
          @click="activeNav = 'about'"
        >About</button>
      </nav>

      <main class="settings-content" :class="{ 'settings-content--cards': props.mode === 'settings' }">
        <template v-if="activeNav === 'general'">
          <!-- Kiri On launch + Window & quit, kanan Container engine + Command line. -->
          <div class="settings-col">
          <div class="section-block">
            <h2 class="section-title">On launch</h2>

            <SettingRow
              label="Start Kova when you sign in"
              :desc="config.minimizeToTray ? 'Opens in the tray, without a window' : 'Opens Kova after you sign in'"
            >
              <UiToggle
                label="Start Kova when you sign in"
                :on="autostartOn"
                :disabled="autostartBusy"
                @click="toggleAutostart"
              />
            </SettingRow>
            <p v-if="autostartError" class="settings-error">{{ autostartError }}</p>

            <SettingRow
              label="Start services"
              desc="Start the saved service selection when Kova opens"
            >
              <UiToggle label="Start services when Kova opens" :on="config.autoStart" @click="setAutoStart(!config.autoStart)" />
            </SettingRow>

            <SettingRow
              label="Remember last session"
              desc="Keep the selected services between launches"
            >
              <UiToggle label="Remember last session" :on="config.rememberSession" @click="setRememberSession(!config.rememberSession)" />
            </SettingRow>

            <SettingRow label="Start projects set to start with Kova" :desc="launchProjectsDesc">
              <UiToggle
                label="Start projects set to start with Kova"
                :on="config.autoStartProjects !== false"
                @click="setAutoStartProjects(config.autoStartProjects === false)"
              />
            </SettingRow>
          </div>


          <div class="section-block">
            <h2 class="section-title">Window &amp; quit</h2>

            <SettingRow
              label="Keep running in the tray when the window closes"
              desc="Closing the window hides Kova in the system tray; projects and services keep running"
            >
              <UiToggle label="Keep running in the tray when the window closes" :on="config.minimizeToTray" @click="setMinimizeToTray(!config.minimizeToTray)" />
            </SettingRow>
            <SettingRow
              label="Stop services when Kova quits"
              desc="Quit Kova from the tray stops MySQL, Redis and the other services first. Data in the volumes is kept."
            >
              <UiToggle label="Stop services when Kova quits" :on="config.stopServicesOnQuit" @click="setStopServicesOnQuit(!config.stopServicesOnQuit)" />
            </SettingRow>
            <SettingRow label="Setup" desc="Check the container engine and mise again, step by step">
              <UiButton @click="setView('onboarding')">Run setup again</UiButton>
            </SettingRow>
          </div>


          <div class="section-block">
            <h2 class="section-title">Appearance</h2>
            <SettingRow label="Interface size" desc="Text and controls in every Kova window. Default matches the design.">
              <UiSegmented
                label="Interface size"
                :model-value="String(config.uiScale ?? 1)"
                :options="[{ value: '0.9', label: 'Compact' }, { value: '1', label: 'Default' }, { value: '1.1', label: 'Large' }]"
                @update:model-value="setUiScale(Number($event))"
              />
            </SettingRow>
          </div>

          </div>
          <div class="settings-col">
          <div class="section-block">
            <h2 class="section-title">Container engine</h2>

            <SettingRow
              v-if="platform === 'windows'"
              label="Engine"
              desc="Auto uses WSL containers, unless Docker still holds your Kova containers"
            >
              <UiSegmented
                label="Container engine"
                :model-value="config.containerRuntime"
                :options="ENGINE_OPTIONS.map((o) => ({ value: o.value, label: o.label, disabled: switchingEngine }))"
                @update:model-value="requestEngine($event as EnginePreference)"
              />
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
                  {{ engineInfo?.engine_problem === 'permission' ? 'No access' : engineInfo?.engine_installed ? 'Stopped' : 'Not installed' }}
                </span>
                <span v-else>Checking…</span>
              </span>
              <span v-if="engineCheckedText && !checkingEngine" class="engine-checked">{{ engineCheckedText }}</span>
              <UiButton :loading="checkingEngine" @click="checkEngine">{{ checkingEngine ? 'Checking…' : 'Check again' }}</UiButton>
            </SettingRow>

            <UiStatusNotice
              v-if="suggestWslc"
              class="engine-suggest"
              tone="info"
              title="WSL containers are available on this PC"
              detail="They run your services without Docker Desktop. Your databases stay in Docker's volumes: dump them to a .sql file first, then use Restore database (Services → Connect → Data) after switching."
            >
              <template #actions>
                <UiButton variant="primary" :disabled="switchingEngine" @click="requestEngine('wslc')">Use WSL containers</UiButton>
              </template>
            </UiStatusNotice>

            <SettingRow v-if="platform === 'windows'" label="WSL version" desc="Built-in containers need WSL 3.0.1 or newer">
              <span class="setting-value-mono">{{ engineInfo?.wsl_version ?? 'Unknown' }}</span>
            </SettingRow>

            <SettingRow
              v-if="engineInfo?.engine !== 'wslc'"
              label="Docker Compose file path"
              desc="Path to generated compose file"
            >
              <span class="setting-value-mono">{{ composePath || 'Not created yet' }}</span>
            </SettingRow>

            <!-- Masalah Docker yang bisa diperbaiki pengguna. -->
            <p v-if="engineInfo?.engine === 'docker' && engineInfo.engine_problem === 'permission'" class="section-hint section-hint--note restore-error">
              Permission denied on the Docker socket. Add your user to the docker group with
              <code>sudo usermod -aG docker $USER</code>, then log out and back in.
            </p>
            <p v-else-if="engineInfo?.engine === 'docker' && engineInfo.engine_problem === 'compose'" class="section-hint section-hint--note restore-error">
              Docker Compose v2 is missing, so services cannot start. Install the docker-compose-plugin package
              (Debian/Ubuntu: <code>sudo apt install docker-compose-plugin</code>).
            </p>
            <p v-if="engineInfo?.engine === 'wslc'" class="section-hint section-hint--note">
              WSL containers keep their own images and volumes, separate from Docker Desktop.
              Databases created under Docker will not appear here until you restore them.
              <template v-if="engineInfo?.elevated">
                Kova is running as Administrator, which uses a separate WSL container session.
              </template>
            </p>
          </div>


          <div v-if="platform === 'windows'" class="section-block">
            <h2 class="section-title">Command line</h2>
            <SettingRow label="kova command" :desc="winCliDesc">
              <UiButton :loading="winCliBusy" :disabled="winCliBusy || !winCli" @click="reinstallWinCli">
                {{ winCli && !winCli.onPath ? 'Add to PATH' : 'Reinstall' }}
              </UiButton>
            </SettingRow>
            <p class="settings-note settings-note--inline">
              Commands: <code>kova up</code>, <code>down</code>, <code>status</code>, <code>logs</code>, <code>shell</code>, <code>restore</code>.
              <code>kova help</code> in a new terminal lists them all.
            </p>
            <p v-if="winCliError" class="settings-error">{{ winCliError }}</p>
          </div>

          <div v-else-if="cli?.supported" class="section-block">
            <h2 class="section-title">Command line</h2>

            <SettingRow label="kova command" :desc="cliDesc">
              <UiButton :loading="cliBusy" :disabled="cliBusy || (cli.installed && !cli.outdated)" @click="installCli">
                {{ cli.installed ? (cli.outdated ? 'Update' : 'Installed') : 'Install' }}
              </UiButton>
            </SettingRow>
            <p v-if="cli.installed && !cli.onPath" class="settings-note">
              The folder is not on your PATH yet. Add this line to your shell profile, then open a new terminal:
              <code>export PATH="$HOME/.local/bin:$PATH"</code>
            </p>
            <p v-if="cliError" class="settings-error">{{ cliError }}</p>
          </div>


          <div class="section-block section-block--danger">
            <h2 class="section-title section-title--red">Danger zone</h2>

            <SettingRow
              label="Reset Kova"
              desc="Removes all projects, environments, profiles and custom services from Kova. Folders and databases stay on disk."
            >
              <UiButton variant="danger" @click="handleReset">Reset</UiButton>
            </SettingRow>
          </div>
          </div>
        </template>
        <template v-else-if="activeNav === 'services'">
          <div class="section-block">
            <h2 class="section-title">Profiles</h2>
            <p class="section-hint">
              Saved sets of services. Change the active profile with the switcher on Services.
            </p>

            <div class="profile-list">
              <div v-for="p in config.profiles" :key="p.id" class="profile-item">
                <template v-if="editingId === p.id">
                  <UiInput
                    v-model="editingName"
                    class="profile-field"
                    :maxlength="40"
                    :aria-label="`New name for profile ${p.name}`"
                    @keyup.enter="commitEditProfile"
                    @keyup.esc="cancelEditProfile"
                  />
                  <div class="profile-actions">
                    <UiButton @click="commitEditProfile">Save</UiButton>
                    <UiButton @click="cancelEditProfile">Cancel</UiButton>
                  </div>
                </template>
                <template v-else>
                  <div class="profile-info">
                    <span class="profile-name">{{ p.name }}</span>
                    <UiChip v-if="p.id === config.activeProfileId" tone="accent" class="profile-badge">active</UiChip>
                    <span class="profile-count">{{ p.serviceIds.length }} {{ p.serviceIds.length === 1 ? 'service' : 'services' }}</span>
                  </div>
                  <div class="profile-actions">
                    <UiButton @click="startEditProfile(p.id, p.name)">Rename</UiButton>
                    <UiButton
                      variant="danger"
                      :disabled="config.profiles.length <= 1"
                      :title="config.profiles.length <= 1 ? 'At least one profile is needed' : 'Delete profile'"
                      @click="handleDeleteProfile(p.id, p.name)"
                    >Delete</UiButton>
                  </div>
                </template>
              </div>
            </div>

            <div class="profile-create">
              <UiInput
                v-model="newProfileName"
                class="profile-field"
                :maxlength="40"
                placeholder="New profile name"
                aria-label="New profile name"
                @keyup.enter="handleCreateProfile"
              />
              <UiButton
                variant="accent"
                :disabled="!newProfileName.trim()"
                @click="handleCreateProfile"
              >Create from selection</UiButton>
            </div>
          </div>

          <div class="section-block">
            <h2 class="section-title">Restore database</h2>
            <p class="section-hint">
              Import a .sql dump into MySQL or PostgreSQL. Kova starts the service, waits until it
              accepts connections, then runs the dump. Useful after switching engines.
            </p>

            <SettingRow label="Service">
              <UiSegmented
                v-model="restoreService"
                label="Restore target"
                :options="[
                  { value: 'mysql', label: 'MySQL', disabled: restoring },
                  { value: 'postgres', label: 'PostgreSQL', disabled: restoring },
                ]"
              />
            </SettingRow>

            <SettingRow label="Dump file" :desc="restoreFile ?? 'No file chosen'">
              <UiButton :disabled="restoring" @click="pickRestoreFile">Choose…</UiButton>
              <UiButton
                :disabled="!restoreFile || restoring"
                @click="runRestore"
              >{{ restoring ? 'Restoring…' : 'Restore' }}</UiButton>
            </SettingRow>

            <p
              v-if="restoreResult"
              class="section-hint section-hint--note"
              :class="{ 'restore-error': !restoreResult.ok }"
            >{{ restoreResult.text }}</p>
          </div>
        </template>


        <template v-else-if="activeNav === 'php_node'">
          <div class="section-block">
            <RuntimeTools v-if="versionCtx.isMise.value" />
            <EmptyState v-else-if="miseCtx" title="mise is not installed" icon="lines">
              Kova uses mise to install and switch PHP, Node, Go and Python. Install it once; your projects' mise.toml files keep working.
              <code class="setting-value-mono">{{ isWindowsHost ? 'winget install jdx.mise' : 'curl https://mise.run | sh' }}</code>
              <template #actions>
                <UiButton v-if="isWindowsHost" variant="primary" :loading="installingMise" @click="installMise">Install mise</UiButton>
                <UiButton variant="quiet" @click="openMiseDocs">I'll install it myself</UiButton>
              </template>
            </EmptyState>
          </div>

          <div class="section-block">
            <h2 class="section-title">Version manager<template v-if="runtimeLoc.loc.value"> in {{ runtimeLoc.loc.value }}</template></h2>

            <!-- Lokasi WSL (canvas L8): mise milik distro, bukan milik Windows. -->
            <template v-if="runtimeLoc.loc.value">
              <SettingRow
                label="mise"
                :desc="wslMiseVersion ? `Manages PHP, Node, Go and Python for projects in ${runtimeLoc.loc.value}` : wslMiseVersion === null ? `Not installed in ${runtimeLoc.loc.value}. Install it from the list above.` : `Checking ${runtimeLoc.loc.value}…`"
              >
                <span class="setting-value-mono">{{ wslMiseVersion ? `v${wslMiseVersion}` : wslMiseVersion === null ? 'not installed' : '…' }}</span>
              </SettingRow>
              <SettingRow
                v-if="wslMiseVersion"
                label="Repair"
                :desc="`Runs mise inside ${runtimeLoc.loc.value}. Reshim fixes missing commands; Clear cache refreshes version lists; Doctor writes a report to the log`"
              >
                <UiButton :disabled="miseBusy" @click="versionCtx.repair('reshim', runtimeLoc.loc.value)">Reshim</UiButton>
                <UiButton :disabled="miseBusy" @click="versionCtx.repair('cache-clear', runtimeLoc.loc.value)">Clear cache</UiButton>
                <UiButton :disabled="miseBusy" @click="versionCtx.repair('doctor', runtimeLoc.loc.value)">Doctor</UiButton>
              </SettingRow>
            </template>
            <template v-else>
            <SettingRow
              label="mise"
              :desc="miseCtx?.provider === 'mise' ? 'Manages PHP, Node, Go and Python per project folder' : 'Not installed. PHP and Node versions need mise.'"
            >
              <span class="setting-value-mono">{{ miseCtx?.miseVersion ? `v${miseCtx.miseVersion}` : 'not installed' }}</span>
            </SettingRow>

            <SettingRow
              v-if="miseCtx?.miseVersion"
              label="Repair"
              desc="Reshim fixes missing commands; Clear cache refreshes version lists; Doctor writes a report to the log"
            >
              <UiButton :disabled="miseBusy" @click="versionCtx.repair('reshim')">Reshim</UiButton>
              <UiButton :disabled="miseBusy" @click="versionCtx.repair('cache-clear')">Clear cache</UiButton>
              <UiButton :disabled="miseBusy" @click="versionCtx.repair('doctor')">Doctor</UiButton>
            </SettingRow>
            </template>

            <p v-if="miseError" class="section-hint section-hint--note">{{ miseError }}</p>
          </div>

        </template>

        <template v-else-if="activeNav === 'about'">
          <div class="section-block">
            <h2 class="section-title">About</h2>
            <div class="about-block">
              <span class="about-name">kova</span>
              <span class="about-desc">Local dev environment manager</span>
              <span class="about-version">v{{ version || '...' }}</span>
            </div>

            <p v-if="updateBannerVisible" class="update-progress-note">
              {{ updatePhase === 'ready' ? `Kova ${latestVersion} is ready to install.` : `Kova ${latestVersion} is available.` }}
              Use the banner at the top of the window.
            </p>
            <div v-else-if="updateAvailable" class="update-banner">
              <span class="ub-text">{{ updatePhase === 'ready' ? 'Ready to install' : 'New version available' }}: <strong>v{{ latestVersion }}</strong></span>
              <span class="ub-actions">
                <UiButton v-if="canInstall" variant="quiet" @click="openReleasePage">Release notes</UiButton>
                <UiButton
                  v-if="canInstall && (updatePhase === 'available' || updatePhase === 'idle')"
                  variant="primary"
                  @click="downloadUpdate"
                >Update</UiButton>
                <UiButton v-else-if="canInstall && updatePhase === 'ready'" variant="primary" @click="applyUpdate">Restart now</UiButton>
                <UiButton v-else-if="!canInstall" variant="primary" @click="openReleasePage">Open release page</UiButton>
              </span>
            </div>

            <p v-if="installOnQuit && updatePhase === 'ready'" class="update-progress-note">
              Kova {{ latestVersion }} will be installed when you quit Kova.
            </p>

            <!-- Hasil lama disembunyikan saat memeriksa; gagal tidak tampil
                 sebagai "latest version". -->
            <UiStatusNotice
              v-if="updateCheckError && !checkingUpdate"
              tone="err"
              title="Could not check for updates"
              :detail="updateCheckError"
              retryable
              @retry="checkUpdate"
            >
              <template #actions>
                <UiButton variant="quiet" @click="openReleasePage">Open release page</UiButton>
              </template>
            </UiStatusNotice>
            <div class="update-row">
              <UiButton :loading="checkingUpdate" :disabled="checkingUpdate" @click="checkUpdate">
                {{ checkingUpdate ? 'Checking…' : 'Check for updates' }}
              </UiButton>
              <span v-if="!checkingUpdate && !updateAvailable && !updateCheckError && lastChecked" class="update-uptodate">
                You have the latest version · checked {{ updateCheckedText }}
              </span>
            </div>
          </div>

          <div class="section-block">
            <h2 class="section-title">Diagnostics</h2>
            <SettingRow label="Diagnostics" desc="Versions, engine and paths, for bug reports">
              <UiButton @click="copyDiagnostics">Copy info</UiButton>
            </SettingRow>
            <SettingRow label="Data folder" :desc="dataDir || '…'">
              <UiButton :disabled="!dataDir" @click="openDataDir">Open</UiButton>
            </SettingRow>
          </div>
        </template>
      </main>
    </div>

    <UiModal
      :open="!!pendingEngine"
      :title="pendingEngine ? `Switch to ${ENGINE_OPTIONS.find((o) => o.value === pendingEngine)?.label}?` : ''"
      size="sm"
      @close="pendingEngine = null"
    >
      <p class="engine-confirm">
        <template v-if="servicesRunning > 0">{{ servicesRunning }} running {{ servicesRunning === 1 ? 'service stays' : 'services stay' }} on the current engine; stop {{ servicesRunning === 1 ? 'it' : 'them' }} first and start {{ servicesRunning === 1 ? 'it' : 'them' }} again after the switch. </template>
        WSL containers and Docker keep separate data, so databases from one engine are not visible on the other. Use Restore database to move them.
      </p>
      <template #footer>
        <UiButton size="md" data-autofocus @click="pendingEngine = null">Cancel</UiButton>
        <UiButton variant="primary" size="md" @click="confirmEngine">Switch engine</UiButton>
      </template>
    </UiModal>
  </div>
</template>

<style scoped>
.engine-checked { font-size: var(--text-xs); color: var(--faint); white-space: nowrap; }
.engine-suggest { margin: var(--space-2) 0; }
.view-settings {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
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
  /* Tanpa latar sendiri: banner update di atasnya tidak meninggalkan strip. */
  background: transparent;
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
  font-size: var(--text-sm);
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

.settings-head { padding: var(--space-5) var(--space-6) var(--space-3); flex: none; }
.settings-h1 { margin: 0; font-size: var(--text-xl); font-weight: 600; color: var(--text); letter-spacing: -0.015em; }

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
  min-width: 0;
}

.section-block--danger {
  margin-top: var(--space-2);
}

.section-title {
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--muted);
  margin-bottom: var(--space-3);
}

/* Di Settings, tiap bagian = satu kartu berisi
   baris dengan judul di atasnya; dua kolom bila cukup lebar. Runtimes
   (mode "runtimes") memakai komponen ini juga, jadi hanya berlaku di sini. */
.settings-content--cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 360px), 1fr));
  align-content: start;
  align-items: start;
  padding: var(--space-8) var(--space-6) var(--space-6);
  gap: calc(var(--space-8) + var(--space-2)) var(--space-5);
}
.settings-col { display: flex; flex-direction: column; gap: calc(var(--space-8) + var(--space-2)); min-width: 0; }
.settings-content--cards .section-block {
  position: relative;
  padding: 0 var(--space-4);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
}
.settings-content--cards .section-block :deep(.setting-row) { padding: var(--space-3) 0; gap: var(--space-4); }
.settings-content--cards .section-block--danger {
  margin-top: 0;
  border-color: color-mix(in srgb, var(--red) 35%, var(--border));
}
/* Section About sudah berupa kartu: blok nama bukan kartu bersarang, dan
   baris update punya ruang bawah seperti SettingRow. */
.settings-content--cards .about-block {
  gap: var(--space-1);
  padding: var(--space-3) 0;
  background: none;
  border: 0;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  border-radius: 0;
}
.settings-content--cards .update-row { margin-top: 0; padding: var(--space-3) 0; }
/* Di dalam kartu: teks pembuka tidak menempel ke tepi atas, catatan terakhir
   punya ruang bawah, dan baris profil menjadi baris bergaris seperti
   SettingRow, bukan kartu di dalam kartu. */
.settings-content--cards .section-hint:not(.section-hint--note) { margin: var(--space-3) 0 0; }
.settings-content--cards .section-hint--note { margin-bottom: var(--space-3); }
.settings-content--cards .profile-list { gap: 0; margin-top: var(--space-1); }
.settings-content--cards .profile-item {
  min-height: 52px;
  padding: var(--space-2) 0;
  background: none;
  border: 0;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  border-radius: 0;
}
.settings-content--cards .profile-name { font-size: var(--text-sm); font-weight: 500; }
.settings-content--cards .profile-create { margin-top: 0; padding: var(--space-3) 0; }
.settings-content--cards .update-banner,
.settings-content--cards .update-progress-note { margin-top: var(--space-3); }
.settings-content--cards .section-title {
  position: absolute;
  left: 0;
  top: calc(-1 * (var(--space-5) + 4px));
  margin: 0;
}

.section-title--red {
  color: color-mix(in srgb, var(--red) 80%, transparent);
}

.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 10px;
  border-radius: 20px;
  font-family: var(--font-sans);
  font-size: var(--text-xs);
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
  font-size: var(--text-xs);
  color: var(--muted);
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

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
  font-size: var(--text-lg);
  font-weight: 700;
  color: var(--accent);
}

.about-desc {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--muted);
}

.about-version {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--faint);
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
  font-size: var(--text-xs);
  color: var(--text);
}

.ub-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}

.update-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-top: var(--space-3);
}

.update-uptodate {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--muted);
}

.placeholder-text {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--faint);
  margin: 0;
  padding: var(--space-4) 0;
}

/* Profiles CRUD */
.section-hint {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--faint);
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
  font-size: var(--text-xs);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-badge {
  /* Chip standar (sama dengan "global" di Runtimes), bukan pil huruf besar. */
  flex-shrink: 0;
  height: 20px;
  padding: 0 6px;
}

.profile-count {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--faint);
  flex-shrink: 0;
}

.profile-actions {
  display: flex;
  gap: var(--space-2);
  flex-shrink: 0;
}

.profile-create {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-top: var(--space-4);
}

.restore-error {
  color: var(--red);
  background: color-mix(in srgb, var(--red) 8%, var(--surface));
}
.settings-note {
  margin: 0 0 var(--space-2);
  font-size: var(--text-xs);
  color: var(--muted);
  line-height: 1.6;
}
.section-hint code { font-family: var(--font-mono); color: var(--text); user-select: text; }
.settings-note code {
  display: block;
  margin-top: 4px;
  font-family: var(--font-mono);
  color: var(--text);
  user-select: text;
}
.settings-note--inline code { display: inline; margin: 0; }
.settings-error {
  margin: 0 0 var(--space-2);
  font-size: var(--text-xs);
  color: var(--red);
}
.profile-field { flex: 1; }
.engine-confirm { margin: 0; line-height: 1.6; }
.update-progress-note { margin: var(--space-2) 0 0; font-size: var(--text-xs); color: var(--faint); }
</style>
