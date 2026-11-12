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
import ProjectsPanel from '@/components/ProjectsPanel.vue'
import PhpExtensionsPanel from '@/components/PhpExtensionsPanel.vue'
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
  setContainerRuntime,
  setVersionProvider,
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

const {
  latestVersion,
  updateAvailable,
  checking: checkingUpdate,
  lastChecked,
  canInstall,
  installing: installingUpdate,
  installPercent,
  installError,
  check: checkUpdate,
  install: installUpdate,
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
        >Projects</button>
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
              desc="Auto uses WSL containers, unless Docker still holds your Servel containers"
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
          <ProjectsPanel />
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
            <PhpExtensionsPanel />
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
              <span class="ub-actions">
                <button v-if="canInstall" class="ub-link" :disabled="installingUpdate" @click="openReleasePage">
                  Catatan rilis
                </button>
                <button v-if="canInstall" class="ub-download" :disabled="installingUpdate" @click="installUpdate">
                  {{ installingUpdate ? 'Memasang…' : 'Update sekarang' }}
                </button>
                <button v-else class="ub-download" @click="openReleasePage">Buka halaman rilis</button>
              </span>
            </div>
            <div v-if="installingUpdate" class="update-progress">
              <div class="up-track">
                <div
                  class="up-fill"
                  :class="{ 'up-fill--unknown': installPercent === null }"
                  :style="installPercent !== null ? { width: installPercent + '%' } : undefined"
                />
              </div>
              <span class="up-text">
                {{ installPercent !== null ? `Mengunduh ${installPercent}%` : 'Mengunduh…' }}
                · Servel akan menutup, memasang, lalu membuka lagi.
              </span>
            </div>
            <p v-if="installError" class="update-error">
              Update otomatis gagal: {{ installError }}. Unduh installer dari halaman rilis.
            </p>

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

.ub-download:disabled,
.ub-link:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.ub-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}

.ub-link {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 8px;
  background: transparent;
  border: none;
  color: var(--muted);
  cursor: pointer;
}

.ub-link:hover:not(:disabled) {
  color: var(--text);
}

.update-progress {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-top: var(--space-3);
}

.up-track {
  height: 6px;
  border-radius: 999px;
  background: var(--surface2);
  overflow: hidden;
}

.up-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 999px;
  transition: width 0.2s var(--ease);
}

.up-fill--unknown {
  width: 35%;
  animation: up-slide 1.2s var(--ease) infinite alternate;
}

@keyframes up-slide {
  from { transform: translateX(-20%); }
  to { transform: translateX(210%); }
}

.up-text {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--muted);
}

.update-error {
  margin-top: var(--space-3);
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--red);
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

.restore-error {
  color: var(--red);
  background: color-mix(in srgb, var(--red) 8%, var(--surface));
}
</style>
