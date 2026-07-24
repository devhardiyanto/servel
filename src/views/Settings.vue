<script setup lang="ts">
import { ref, inject, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { ask } from '@tauri-apps/plugin-dialog'
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import { useUpdateCheck } from '@/composables/useUpdateCheck'
import {
  useSites,
  isValidIpv4,
  isValidDomain,
  type SitesStatus,
  type DiffResult,
} from '@/composables/useSites'
import SettingRow from '@/components/SettingRow.vue'
import type { PrereqStatus } from '@/types/prereq'

type SettingsNav = 'general' | 'services' | 'sites' | 'php_node' | 'about'

const setView = inject(SetViewKey)!

const {
  config,
  loaded,
  load,
  setAutoStart,
  setRememberSession,
  setMinimizeToTray,
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
const siteError = ref('')
const editingSiteId = ref<string | null>(null)
const editDomain = ref('')
const editIp = ref('')

const sitesStatus = ref<SitesStatus | null>(null)
const sitesBusy = ref(false) // apply/restore sedang jalan (UAC)
const applyError = ref('')
const showDiffModal = ref(false)
const pendingDiff = ref<DiffResult | null>(null)
const selectedBackup = ref('')

async function refreshSitesStatus(): Promise<void> {
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
  addSite(d, ip)
  newDomain.value = ''
  newIp.value = '127.0.0.1'
  void refreshSitesStatus()
}

function startEditSite(id: string, domain: string, ip: string): void {
  editingSiteId.value = id
  editDomain.value = domain
  editIp.value = ip
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
  if (editingSiteId.value) updateSite(editingSiteId.value, { domain: d, ip })
  editingSiteId.value = null
  void refreshSitesStatus()
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
}

function handleToggleSite(id: string): void {
  toggleSite(id)
  void refreshSitesStatus()
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
const dockerRunning = ref<boolean | null>(null)
const checkingDocker = ref(false)
const version = ref<string>('')

async function fetchComposePath(): Promise<void> {
  try {
    composePath.value = await invoke<string>('get_compose_path')
  } catch {
    composePath.value = '—'
  }
}

async function checkDocker(): Promise<void> {
  checkingDocker.value = true
  try {
    const result = await invoke<PrereqStatus>('check_prerequisites')
    dockerRunning.value = result.docker_running
  } catch {
    dockerRunning.value = false
  } finally {
    checkingDocker.value = false
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
  await fetchComposePath()
  await checkDocker()
  await refreshSitesStatus()
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
            <div class="section-title">DOCKER</div>

            <SettingRow
              label="Docker Compose file path"
              desc="Path to generated compose file"
            >
              <span class="setting-value-mono">{{ composePath || '—' }}</span>
            </SettingRow>

            <SettingRow label="Docker status">
              <span
                class="status-pill"
                :class="dockerRunning === true ? 'status-pill--green' : 'status-pill--red'"
              >
                <span class="status-dot"></span>
                <span v-if="dockerRunning === true">Running</span>
                <span v-else-if="dockerRunning === false">Stopped</span>
                <span v-else>—</span>
              </span>
              <button
                class="action-btn"
                :disabled="checkingDocker"
                @click="checkDocker"
              >{{ checkingDocker ? '...' : 'Refresh' }}</button>
            </SettingRow>
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
                  </div>
                  <div class="profile-actions">
                    <button class="action-btn" @click="startEditSite(s.id, s.domain, s.ip)">Edit</button>
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
              <button
                class="action-btn action-btn--accent"
                :disabled="!newDomain.trim()"
                @click="handleAddSite"
              >Tambah</button>
            </div>
            <p v-if="siteError" class="site-error">{{ siteError }}</p>
            <p v-if="applyError && !showDiffModal" class="site-error">{{ applyError }}</p>
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
            <div class="section-title">PHP &amp; NODE</div>
            <p class="placeholder-text">PHP &amp; Node configuration — coming soon.</p>
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
  border-radius: 4px;
  transition: color 0.1s, background 0.1s;
}

.app-strip__back:hover {
  color: var(--text);
  background: var(--surface2);
}

.app-strip__title {
  font-family: var(--font-mono);
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
  font-family: var(--font-mono);
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
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.1em;
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
  border-radius: 10px;
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
  font-family: var(--font-mono);
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
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 4px 12px;
  border-radius: 4px;
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
  border-radius: 8px;
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
  border-radius: 6px;
}

.ub-text {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--text);
}

.ub-download {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 4px 12px;
  background: color-mix(in srgb, var(--accent) 18%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  border-radius: 4px;
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
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 4px 12px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 4px;
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
  border-radius: 6px;
}

.profile-info {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.profile-name {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-badge {
  font-family: var(--font-mono);
  font-size: 9px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  padding: 1px 6px;
  border-radius: 10px;
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
  color: var(--accent);
  flex-shrink: 0;
}

.profile-count {
  font-family: var(--font-mono);
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
  font-family: var(--font-mono);
  font-size: 12px;
  padding: 5px 8px;
  border-radius: 4px;
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
  border-radius: 6px;
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
  border-radius: 8px;
  padding: var(--space-6);
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.modal-title {
  font-family: var(--font-mono);
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
  border-radius: 6px;
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
</style>
