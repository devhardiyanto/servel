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
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import { useUpdateCheck } from '@/composables/useUpdateCheck'
import RuntimeTools from '@/components/RuntimeTools.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import SettingRow from '@/components/SettingRow.vue'
import { ENGINE_LABEL, type EnginePreference, type PrereqStatus } from '@/types/prereq'
import { useEngineStatus } from '@/composables/useEngineStatus'

type SettingsNav = 'general' | 'services' | 'sites' | 'php_node' | 'about'

const setView = inject(SetViewKey)!

const {
  config,
  loaded,
  load,
  setAutoStart,
  setRememberSession,
  setMinimizeToTray,
  setStopServicesOnQuit,
  setContainerRuntime,
  createProfile,
  renameProfile,
  deleteProfile,
  reset,
} = useConfig()

// v3: Settings tampil sebagai dua halaman di navigasi kiri: "settings"
// (General, Services, About) dan "runtimes" (PHP/Node, runtime lain, extension).
const props = withDefaults(defineProps<{ mode?: 'settings' | 'runtimes' }>(), { mode: 'settings' })
const activeNav = ref<SettingsNav>(props.mode === 'runtimes' ? 'php_node' : 'general')

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
  canInstall,
  phase: updatePhase,
  bannerVisible: updateBannerVisible,
  check: checkUpdate,
  download: downloadUpdate,
  apply: applyUpdate,
  openReleasePage,
} = useUpdateCheck()

const composePath = ref<string>('')
const engineInfo = ref<PrereqStatus | null>(null)
const checkingEngine = ref(false)
const switchingEngine = ref(false)
const platform = ref<string>('windows')
const version = ref<string>('')

// v3.5: Diagnostics di About.
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
    toast.show('Could not copy the diagnostics.', { tone: 'info' })
  }
}
async function openDataDir(): Promise<void> {
  try {
    await invoke('project_open_in', { target: 'folder', path: dataDir.value })
  } catch (err) {
    toast.show(String(err), { tone: 'info' })
  }
}

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

// v3.5: state "mise is not installed" di Runtimes.
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

// v3.7.2: kembali ke Kova setelah memasang WSL/Docker di luar Kova memeriksa
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
  const { push } = useLogs('KOVA')
  push({ ts: new Date().toTimeString().slice(0, 8), src: 'KOVA', text: 'settings reset to defaults' })
}

// "Start Kova when you sign in" (v2.11): disimpan oleh OS, bukan di config.
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

onMounted(async () => {
  invoke<CliStatus>('cli_status').then((v) => (cli.value = v)).catch(() => {})
  invoke<boolean>('autostart_get').then((v) => (autostartOn.value = v)).catch(() => {})
  if (!loaded.value) await load()
  try {
    platform.value = await invoke<string>('get_platform')
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

      <main class="settings-content">
        <template v-if="activeNav === 'general'">
          <div class="section-block">
            <div class="section-title">ON LAUNCH</div>

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

            <SettingRow
              label="Projects"
              desc="Each project has its own &quot;Start this project when Kova opens&quot; option in Projects → Edit"
            >
              <UiButton variant="quiet" @click="setView('projects')">Open Projects</UiButton>
            </SettingRow>
          </div>

          <div class="section-block">
            <div class="section-title">WINDOW</div>

            <SettingRow
              label="Minimize to tray on close"
              desc="Keep Kova running in the system tray instead of quitting when the window closes"
            >
              <UiToggle label="Minimize to tray on close" :on="config.minimizeToTray" @click="setMinimizeToTray(!config.minimizeToTray)" />
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

          <div v-if="cli?.supported" class="section-block">
            <div class="section-title">TERMINAL</div>

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

          <div class="section-block">
            <div class="section-title">CONTAINER ENGINE</div>

            <SettingRow
              v-if="platform === 'windows'"
              label="Engine"
              desc="Auto uses WSL containers, unless Docker still holds your Kova containers"
            >
              <UiSegmented
                label="Container engine"
                :model-value="config.containerRuntime"
                :options="ENGINE_OPTIONS.map((o) => ({ value: o.value, label: o.label, disabled: switchingEngine }))"
                @update:model-value="chooseEngine($event as EnginePreference)"
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
                  {{ engineInfo?.engine_installed ? 'Stopped' : 'Not installed' }}
                </span>
                <span v-else>—</span>
              </span>
              <UiButton
                :disabled="checkingEngine"
                @click="checkEngine"
              >{{ checkingEngine ? '...' : 'Refresh' }}</UiButton>
            </SettingRow>

            <SettingRow v-if="platform === 'windows'" label="WSL version" desc="Built-in containers need WSL 3.0.1 or newer">
              <span class="setting-value-mono">{{ engineInfo?.wsl_version ?? '—' }}</span>
            </SettingRow>

            <SettingRow
              v-if="platform === 'windows'"
              label="Terminal command"
              desc="Added to your PATH by the installer — kova up · down · status · logs · shell · restore (the old servel command still works until v4.0)"
            >
              <span class="setting-value-mono">kova help</span>
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
                Kova is running as Administrator, which uses a separate WSL container session.
              </template>
            </p>
          </div>

          <div class="section-block section-block--danger">
            <div class="section-title section-title--red">DANGER ZONE</div>

            <SettingRow
              label="Reset all settings"
              desc="Restore defaults and clear saved selection"
            >
              <UiButton variant="danger" @click="handleReset">Reset</UiButton>
            </SettingRow>
          </div>
        </template>

        <template v-else-if="activeNav === 'services'">
          <div class="section-block">
            <div class="section-title">PROFILES</div>
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
            <div class="section-title">RESTORE DATABASE</div>
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
            <div class="section-title">VERSION MANAGER</div>

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

            <p v-if="miseError" class="section-hint section-hint--note">{{ miseError }}</p>
          </div>

        </template>

        <template v-else-if="activeNav === 'about'">
          <div class="section-block">
            <div class="section-title">ABOUT</div>
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

            <div class="update-row">
              <UiButton :disabled="checkingUpdate" @click="checkUpdate">
                {{ checkingUpdate ? 'Checking…' : 'Check for updates' }}
              </UiButton>
              <span v-if="!updateAvailable && lastChecked" class="update-uptodate">You have the latest version</span>
            </div>
          </div>

          <div class="section-block">
            <div class="section-title">DIAGNOSTICS</div>
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
  font-size: var(--text-lg);
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
  font-size: var(--text-xs);
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
  /* Tanpa latar sendiri: banner update di atasnya tidak meninggalkan strip (v3.6.1). */
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
  font-size: var(--text-xs);
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
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.07em;
  color: var(--faint);
  margin-bottom: var(--space-3);
}

.section-title--red {
  color: color-mix(in srgb, var(--red) 80%, transparent);
}

/* Toggle */




/* Status pill */
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

/* Placeholder */
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
  /* v3.7.0: chip standar (sama dengan "global" di Runtimes), bukan pil huruf besar. */
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
.settings-note code {
  display: block;
  margin-top: 4px;
  font-family: var(--font-mono);
  color: var(--text);
  user-select: text;
}
.settings-error {
  margin: 0 0 var(--space-2);
  font-size: var(--text-xs);
  color: var(--red);
}
.profile-field { flex: 1; }
.update-progress-note { margin: var(--space-2) 0 0; font-size: var(--text-xs); color: var(--faint); }
</style>
