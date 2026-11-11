<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ask, open as openDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useConfig } from '@/composables/useConfig'
import { useVersionContext } from '@/composables/useVersionContext'
import {
  useProjectRuntime,
  isActive,
  type LegacyStatus,
  type SiteState,
} from '@/composables/useProjectRuntime'
import type { Project } from '@/types/mise'

const { config, addProject, removeProject, updateProject } = useConfig()
const versionCtx = useVersionContext()
const { stateOf, infoOf, start, stop, restart, forget, detect } = useProjectRuntime()

const projects = computed<Project[]>(() => config.value.projects)
const errors = ref<Record<string, string>>({})
const busy = ref<Record<string, boolean>>({})
const panelError = ref('')

const STATE_LABEL: Record<SiteState, string> = {
  stopped: 'stopped',
  starting: 'starting',
  running: 'running',
  unresponsive: 'running, port not answering',
  exited: 'stopped',
}

function stateLabel(p: Project): string {
  const info = infoOf(p.id)
  if (info?.state === 'exited') {
    return info.exitCode !== null ? `exited (code ${info.exitCode})` : 'exited'
  }
  return STATE_LABEL[stateOf(p.id)]
}

function url(p: Project): string {
  return `http://localhost:${p.port}`
}

function takenPorts(exceptId?: string): number[] {
  return projects.value.filter((p) => p.id !== exceptId && (p.port ?? 0) > 0).map((p) => p.port as number)
}

function canRun(p: Project): boolean {
  return !!p.path && !!p.command?.trim() && (p.port ?? 0) >= 1024
}

function missing(p: Project): string {
  if (!p.path) return 'Choose the project folder first.'
  if (!p.command?.trim()) return 'No command yet. Open Edit and fill it in.'
  if ((p.port ?? 0) < 1024) return 'Set a port (1024–65535) in Edit.'
  return ''
}

async function pickFolder(title: string): Promise<string | null> {
  try {
    const selected = await openDialog({ directory: true, multiple: false, title })
    return typeof selected === 'string' ? selected : null
  } catch (err) {
    panelError.value = String(err)
    return null
  }
}

// --- Tambah / pilih folder -------------------------------------------------
async function handleAdd(): Promise<void> {
  panelError.value = ''
  const path = await pickFolder('Add project folder')
  if (!path) return
  try {
    const s = await detect(path, takenPorts())
    const before = projects.value.length
    const project = await addProject(path, {
      name: s.name,
      kind: s.detection.label,
      command: s.detection.command,
      port: s.port,
      autoStart: false,
    })
    await versionCtx.refresh()
    if (!project) return
    if (projects.value.length === before) {
      panelError.value = `"${project.name}" is already in the list.`
      return
    }
    // Buka form supaya perintah dan port usulan bisa diperiksa dulu.
    startEdit(project)
  } catch (err) {
    panelError.value = String(err)
  }
}

/** Project hasil migrasi site lama: belum punya folder. */
async function handleChooseFolder(p: Project): Promise<void> {
  const path = await pickFolder(`Choose the folder for ${p.name}`)
  if (!path) return
  try {
    const s = await detect(path, takenPorts(p.id))
    await updateProject(p.id, {
      path: path.replace(/[\\/]+$/, ''),
      kind: s.detection.label,
      command: s.detection.command,
      // Port yang sudah diisi di versi lama dipertahankan.
      port: (p.port ?? 0) >= 1024 ? p.port : s.port,
    })
    await versionCtx.refresh()
    const updated = projects.value.find((x) => x.id === p.id)
    if (updated) startEdit(updated)
  } catch (err) {
    errors.value = { ...errors.value, [p.id]: String(err) }
  }
}

// --- Edit ------------------------------------------------------------------
const editingId = ref<string | null>(null)
const form = ref({ name: '', command: '', port: '', autoStart: false, profileId: '' })
const formError = ref('')

function startEdit(p: Project): void {
  editingId.value = p.id
  formError.value = ''
  form.value = {
    name: p.name,
    command: p.command ?? '',
    port: (p.port ?? 0) > 0 ? String(p.port) : '',
    autoStart: p.autoStart ?? false,
    profileId: p.profileId ?? '',
  }
}

function cancelEdit(): void {
  editingId.value = null
  formError.value = ''
}

async function redetect(p: Project): Promise<void> {
  if (!p.path) return
  try {
    const s = await detect(p.path, takenPorts(p.id))
    form.value.command = s.detection.command
    if (!form.value.port) form.value.port = String(s.port)
    await updateProject(p.id, { kind: s.detection.label })
    formError.value = s.detection.command ? '' : 'Project type not recognized. Type the command yourself.'
  } catch (err) {
    formError.value = String(err)
  }
}

async function saveEdit(p: Project, thenStart = false): Promise<void> {
  const name = form.value.name.trim()
  if (!name) {
    formError.value = 'Name is required.'
    return
  }
  let port = 0
  const rawPort = form.value.port.trim()
  if (rawPort !== '') {
    if (!/^\d{1,5}$/.test(rawPort) || Number(rawPort) < 1024 || Number(rawPort) > 65535) {
      formError.value = 'Port must be 1024–65535.'
      return
    }
    port = Number(rawPort)
    const clash = projects.value.find((x) => x.id !== p.id && x.port === port)
    if (clash) {
      formError.value = `Port ${port} is already used by "${clash.name}".`
      return
    }
  }
  await updateProject(p.id, {
    name,
    command: form.value.command.trim(),
    port,
    autoStart: form.value.autoStart,
    profileId: form.value.profileId || null,
  })
  editingId.value = null
  formError.value = ''
  if (thenStart) {
    const updated = projects.value.find((x) => x.id === p.id)
    if (updated) await handleStart(updated)
  }
}

// --- Start / Stop / Open ---------------------------------------------------
async function handleStart(p: Project): Promise<void> {
  errors.value = { ...errors.value, [p.id]: '' }
  busy.value = { ...busy.value, [p.id]: true }
  try {
    await start(p)
  } catch (err) {
    errors.value = { ...errors.value, [p.id]: String(err) }
  } finally {
    busy.value = { ...busy.value, [p.id]: false }
  }
}

async function handleStop(p: Project): Promise<void> {
  busy.value = { ...busy.value, [p.id]: true }
  try {
    await stop(p.id)
  } catch (err) {
    errors.value = { ...errors.value, [p.id]: String(err) }
  } finally {
    busy.value = { ...busy.value, [p.id]: false }
  }
}

async function handleRestart(p: Project): Promise<void> {
  errors.value = { ...errors.value, [p.id]: '' }
  busy.value = { ...busy.value, [p.id]: true }
  try {
    await restart(p)
  } catch (err) {
    errors.value = { ...errors.value, [p.id]: String(err) }
  } finally {
    busy.value = { ...busy.value, [p.id]: false }
  }
}

async function handleOpen(p: Project): Promise<void> {
  try {
    await openUrl(url(p))
  } catch {
    window.open(url(p), '_blank', 'noopener')
  }
}

async function handleRemove(p: Project): Promise<void> {
  let ok = false
  try {
    ok = await ask(`Remove "${p.name}" from Servel? Files and mise.toml are not touched.`, {
      title: 'Remove project',
      kind: 'warning',
    })
  } catch {
    ok = window.confirm(`Remove "${p.name}"?`)
  }
  if (!ok) return
  if (isActive(stateOf(p.id))) await stop(p.id)
  await removeProject(p.id)
  await forget(p.id)
  await versionCtx.refresh()
}

// --- Sisa fitur domain .test -----------------------------------------------
const legacy = ref<LegacyStatus>({ hostsBlock: false, cert: false })
const legacyBusy = ref(false)
const legacyError = ref('')

async function refreshLegacy(): Promise<void> {
  try {
    legacy.value = await invoke<LegacyStatus>('legacy_status')
  } catch (err) {
    console.error('[legacy_status]', err)
  }
}

async function cleanup(command: 'legacy_cleanup_hosts' | 'legacy_remove_cert'): Promise<void> {
  legacyBusy.value = true
  legacyError.value = ''
  try {
    await invoke<void>(command)
  } catch (err) {
    legacyError.value = String(err)
  } finally {
    await refreshLegacy()
    legacyBusy.value = false
  }
}

onMounted(() => {
  void refreshLegacy()
})
</script>

<template>
  <div class="pp">
    <div class="pp-title">PROJECTS</div>
    <p class="pp-hint">
      Each project is a folder. Servel runs its dev server with the PHP/Node versions from the project's
      mise.toml and gives it a fixed address on localhost. The same list feeds "Versions for" on the Dashboard.
    </p>

    <div v-if="legacy.hostsBlock || legacy.cert" class="pp-legacy">
      <div class="pp-legacy__head">Left over from the removed .test domains</div>
      <div v-if="legacy.hostsBlock" class="pp-legacy__row">
        <span>Your hosts file still has Servel's .test entries.</span>
        <button class="pp-btn" :disabled="legacyBusy" @click="cleanup('legacy_cleanup_hosts')">
          Remove from hosts
        </button>
      </div>
      <div v-if="legacy.cert" class="pp-legacy__row">
        <span>The local HTTPS certificate is still trusted.</span>
        <button class="pp-btn" :disabled="legacyBusy" @click="cleanup('legacy_remove_cert')">
          Remove certificate
        </button>
      </div>
      <p v-if="legacyError" class="pp-error">{{ legacyError }}</p>
    </div>

    <div class="pp-list">
      <div v-for="p in projects" :key="p.id" class="pp-card">
        <div class="pp-row">
          <div class="pp-who">
            <div class="pp-name">
              {{ p.name }}
              <span v-if="p.kind" class="pp-kind">{{ p.kind }}</span>
              <span v-if="p.autoStart" class="pp-kind">auto-start</span>
            </div>
            <div class="pp-path" :title="p.path">{{ p.path || 'Folder not chosen yet (from an old .test site)' }}</div>
          </div>

          <template v-if="canRun(p)">
            <button
              v-if="stateOf(p.id) === 'running' || stateOf(p.id) === 'unresponsive'"
              class="pp-url"
              :title="`Open ${url(p)}`"
              @click="handleOpen(p)"
            >localhost:{{ p.port }}</button>
            <span v-else class="pp-url pp-url--off">localhost:{{ p.port }}</span>
            <span class="pp-state" :class="`pp-state--${stateOf(p.id)}`">{{ stateLabel(p) }}</span>
          </template>

          <div class="pp-actions">
            <button v-if="!p.path" class="pp-btn pp-btn--accent" @click="handleChooseFolder(p)">Choose folder</button>
            <template v-else-if="canRun(p)">
              <button
                v-if="isActive(stateOf(p.id))"
                class="pp-btn"
                :disabled="busy[p.id]"
                @click="handleStop(p)"
              >Stop</button>
              <button v-else class="pp-btn pp-btn--accent" :disabled="busy[p.id]" @click="handleStart(p)">
                Start
              </button>
              <button
                class="pp-btn"
                :disabled="busy[p.id] || !isActive(stateOf(p.id))"
                :title="isActive(stateOf(p.id)) ? 'Stop, then start again' : 'Project is not running'"
                @click="handleRestart(p)"
              >Restart</button>
            </template>
            <button v-if="p.path" class="pp-btn pp-btn--ghost" @click="editingId === p.id ? cancelEdit() : startEdit(p)">
              Edit
            </button>
            <button class="pp-btn pp-btn--ghost pp-btn--red" @click="handleRemove(p)">Remove</button>
          </div>
        </div>

        <p v-if="!canRun(p) && editingId !== p.id" class="pp-note">{{ missing(p) }}</p>
        <p v-if="errors[p.id]" class="pp-error">{{ errors[p.id] }}</p>
        <p v-else-if="infoOf(p.id)?.state === 'exited' && infoOf(p.id)?.lastLine" class="pp-error pp-error--mono">
          {{ infoOf(p.id)?.lastLine }}
        </p>
        <p v-else-if="stateOf(p.id) === 'unresponsive'" class="pp-note">
          The process is running but nothing answers on port {{ p.port }}. The app may be listening on a different
          port; check the Logs, then set that port in Edit.
        </p>

        <div v-if="editingId === p.id" class="pp-form">
          <label :for="`pp-name-${p.id}`">Name</label>
          <input :id="`pp-name-${p.id}`" v-model="form.name" class="pp-input" />

          <label :for="`pp-cmd-${p.id}`">Command</label>
          <div class="pp-inline">
            <input
              :id="`pp-cmd-${p.id}`"
              v-model="form.command"
              class="pp-input pp-input--mono"
              placeholder="e.g. npm run dev -- --port {port}"
              spellcheck="false"
            />
            <button class="pp-btn" @click="redetect(p)">Detect</button>
          </div>

          <label :for="`pp-port-${p.id}`">Port</label>
          <input
            :id="`pp-port-${p.id}`"
            v-model="form.port"
            class="pp-input pp-input--mono pp-input--port"
            inputmode="numeric"
            placeholder="8000"
          />

          <label :for="`pp-profile-${p.id}`">Services</label>
          <select :id="`pp-profile-${p.id}`" v-model="form.profileId" class="pp-input">
            <option value="">No service profile</option>
            <option v-for="pr in config.profiles" :key="pr.id" :value="pr.id">Profile: {{ pr.name }}</option>
          </select>

          <label :for="`pp-auto-${p.id}`">On launch</label>
          <label class="pp-check">
            <input :id="`pp-auto-${p.id}`" v-model="form.autoStart" type="checkbox" />
            Start this project when Servel opens
          </label>

          <p class="pp-form__hint">
            <code>{port}</code> in the command is replaced by the port, and the <code>PORT</code> environment variable
            is set to it. Leave the command empty to use this project only for versions and services. Changes apply
            the next time the project starts.
          </p>
          <p v-if="formError" class="pp-error pp-form__wide">{{ formError }}</p>
          <div class="pp-form__actions">
            <button class="pp-btn pp-btn--ghost" @click="cancelEdit">Cancel</button>
            <button class="pp-btn" @click="saveEdit(p)">Save</button>
            <button
              v-if="!isActive(stateOf(p.id))"
              class="pp-btn pp-btn--accent"
              @click="saveEdit(p, true)"
            >Save &amp; start</button>
          </div>
        </div>
      </div>

      <p v-if="projects.length === 0" class="pp-hint">No projects yet. Add a folder to get started.</p>
    </div>

    <p v-if="panelError" class="pp-error">{{ panelError }}</p>
    <div><button class="pp-btn pp-btn--accent" @click="handleAdd">+ Add project folder</button></div>
  </div>
</template>

<style scoped>
.pp {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.pp-title {
  font-family: var(--font-sans);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--dim);
}

.pp-hint,
.pp-note {
  font-family: var(--font-sans);
  font-size: 12px;
  line-height: 1.55;
  color: var(--muted);
  margin: 0;
}

.pp-note {
  color: var(--amber);
}

.pp-error {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--red);
  margin: 0;
  overflow-wrap: anywhere;
}

.pp-error--mono {
  font-family: var(--font-mono);
  font-size: 11px;
}

.pp-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.pp-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
}

.pp-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2) var(--space-4);
}

.pp-who {
  flex: 1 1 220px;
  min-width: 0;
}

.pp-name {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.pp-kind {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 400;
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--surface2);
  color: var(--muted);
}

.pp-path {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pp-url {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--accent);
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  text-decoration: underline;
  text-decoration-color: color-mix(in srgb, var(--accent) 40%, transparent);
  text-underline-offset: 3px;
  white-space: nowrap;
}

.pp-url--off {
  color: var(--dim);
  text-decoration: none;
  cursor: default;
}

.pp-state {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-family: var(--font-mono);
  font-size: 11px;
  white-space: nowrap;
  color: var(--dim);
}

.pp-state::before {
  content: '';
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}

.pp-state--running {
  color: var(--green);
}

.pp-state--starting,
.pp-state--unresponsive {
  color: var(--amber);
}

.pp-state--exited {
  color: var(--red);
}

.pp-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.pp-btn {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 12px;
  background: var(--surface2);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-xs);
  color: var(--text);
  cursor: pointer;
  white-space: nowrap;
  transition: border-color 0.1s, background 0.1s;
}

.pp-btn:hover:not(:disabled) {
  border-color: var(--dim);
}

.pp-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.pp-btn:focus-visible,
.pp-url:focus-visible,
.pp-input:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.pp-btn--accent {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  color: var(--accent);
}

.pp-btn--ghost {
  background: transparent;
  color: var(--muted);
}

.pp-btn--red:hover:not(:disabled) {
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 45%, transparent);
}

.pp-form {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr);
  gap: var(--space-2) var(--space-3);
  align-items: center;
  padding-top: var(--space-3);
  border-top: 1px solid var(--border);
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--dim);
}

.pp-inline {
  display: flex;
  gap: 6px;
  min-width: 0;
}

.pp-input {
  flex: 1;
  min-width: 0;
  font-family: var(--font-sans);
  font-size: 12px;
  padding: 5px 10px;
  background: var(--bg);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-xs);
  color: var(--text);
}

.pp-input--mono {
  font-family: var(--font-mono);
  font-size: 11px;
}

.pp-input--port {
  max-width: 120px;
}

.pp-check {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--text);
}

.pp-form__hint,
.pp-form__wide,
.pp-form__actions {
  grid-column: 1 / -1;
}

.pp-form__hint {
  margin: 0;
  line-height: 1.55;
  color: var(--dim);
}

.pp-form__hint code {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
}

.pp-form__actions {
  display: flex;
  justify-content: flex-end;
  gap: 6px;
}

.pp-legacy {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-md);
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--muted);
}

.pp-legacy__head {
  font-weight: 600;
  color: var(--text);
}

.pp-legacy__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}
</style>
