<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import UiMenu, { type MenuItem } from '@/components/ui/UiMenu.vue'
import UiModal from '@/components/ui/UiModal.vue'
import UiDrawer from '@/components/ui/UiDrawer.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiChip from '@/components/ui/UiChip.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusDot from '@/components/ui/StatusDot.vue'
import { invoke } from '@tauri-apps/api/core'
import { useToast } from '@/composables/useToast'
import { ref, computed, inject } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useConfig } from '@/composables/useConfig'
import { useVersionContext } from '@/composables/useVersionContext'
import {
  useProjectRuntime,
  isActive,
  type SiteState,
} from '@/composables/useProjectRuntime'
import type { Project } from '@/types/mise'
import { parseEnvText, envToText } from '@/utils/envText'
import { useLogFocus } from '@/composables/useLogFocus'
import { SetViewKey } from '@/types/navigation'

const setView = inject(SetViewKey, () => {})
const logFocus = useLogFocus()

/** Buka Logs → Projects dengan filter project ini. */
function viewLog(p: Project): void {
  logFocus.focusProject(p.name)
  setView('logs')
}

const { config, addProject, removeProject, updateProject } = useConfig()
const versionCtx = useVersionContext()
const { stateOf, infoOf, start, stop, restart, forget, detect } = useProjectRuntime()

const projects = computed<Project[]>(() => config.value.projects)
const errors = ref<Record<string, string>>({})
const busy = ref<Record<string, boolean>>({})
const panelError = ref('')
// v3: daftar di kiri, detail project terpilih ditampilkan ProjectsView di kanan.
defineProps<{ selectedId?: string | null }>()
const emit = defineEmits<{ select: [id: string] }>()

const STATE_LABEL: Record<SiteState, string> = {
  stopped: 'stopped',
  starting: 'starting',
  running: 'running',
  unresponsive: 'not answering',
  exited: 'stopped',
}

function stateLabel(p: Project): string {
  const info = infoOf(p.id)
  if (info?.state === 'exited') {
    return info.exitCode !== null ? `exited (${info.exitCode})` : 'exited'
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
const form = ref({ name: '', command: '', port: '', autoStart: false, profileId: '', env: '', startServices: false })
const formError = ref('')

function startEdit(p: Project): void {
  editingId.value = p.id
  formError.value = ''
  form.value = {
    name: p.name,
    command: p.command ?? '',
    port: (p.port ?? 0) > 0 ? String(p.port) : '',
    autoStart: p.autoStart ?? false,
    env: envToText(p.env),
    profileId: p.profileId ?? '',
    startServices: p.startServices ?? false,
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
  const parsed = parseEnvText(form.value.env)
  if (parsed.error) {
    formError.value = `Environment — ${parsed.error}`
    return
  }
  await updateProject(p.id, {
    env: parsed.env,
    name,
    command: form.value.command.trim(),
    port,
    autoStart: form.value.autoStart,
    profileId: form.value.profileId || null,
    // Tanpa profil opsi ini tidak berarti; simpan mati supaya tidak menyala diam-diam nanti.
    startServices: form.value.profileId ? form.value.startServices : false,
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

// --- Hapus: dialog konfirmasi, fokus awal di Cancel -------------------------
const removing = ref<Project | null>(null)
const removeBusy = ref(false)

async function confirmRemove(): Promise<void> {
  const p = removing.value
  if (!p) return
  removeBusy.value = true
  try {
    if (isActive(stateOf(p.id))) await stop(p.id)
    await removeProject(p.id)
    await forget(p.id)
    await versionCtx.refresh()
    if (editingId.value === p.id) cancelEdit()
    removing.value = null
  } catch (err) {
    errors.value = { ...errors.value, [p.id]: String(err) }
    removing.value = null
  } finally {
    removeBusy.value = false
  }
}

// --- Menu ⋯ ------------------------------------------------------------------
const toast = useToast()

function menuItems(p: Project): MenuItem[] {
  const hasFolder = !!p.path
  const items: MenuItem[] = [
    { id: 'folder', label: 'Open folder', disabled: !hasFolder },
    { id: 'terminal', label: 'Terminal here', disabled: !hasFolder },
    { id: 'vscode', label: 'Open in VS Code', disabled: !hasFolder },
  ]
  if ((p.port ?? 0) > 0) items.push({ id: 'copy-url', label: 'Copy URL', hint: `:${p.port}` })
  items.push({ id: 'settings', label: 'Project settings…', separator: true, disabled: !hasFolder })
  items.push({ id: 'remove', label: 'Remove project…', separator: true, danger: true })
  return items
}

async function onMenu(p: Project, id: string): Promise<void> {
  errors.value = { ...errors.value, [p.id]: '' }
  try {
    if (id === 'folder' || id === 'terminal' || id === 'vscode') {
      await invoke('project_open_in', { target: id, path: p.path })
    } else if (id === 'copy-url') {
      await navigator.clipboard.writeText(url(p))
      toast.show(`Copied ${url(p)}`)
    } else if (id === 'settings') {
      startEdit(p)
    } else if (id === 'remove') {
      removing.value = p
    }
  } catch (err) {
    errors.value = { ...errors.value, [p.id]: String(err) }
  }
}

const editingPath = computed({ get: () => editing.value?.path ?? '', set: () => {} })
const editing = computed(() => projects.value.find((p) => p.id === editingId.value) ?? null)
const profileOptions = computed(() => [
  { value: '', label: 'No service profile' },
  ...config.value.profiles.map((pr) => ({ value: pr.id, label: `Profile: ${pr.name}` })),
])

function dotState(p: Project): SiteState {
  return stateOf(p.id)
}
</script>

<template>
  <div class="pp">
    <div class="pp-title">PROJECTS</div>
    <p class="pp-hint">
      Each project is a folder. Kova runs its dev server with the PHP/Node versions from the project's
      mise.toml and gives it a fixed address on localhost. The same list feeds "Versions for" on Runtimes.
    </p>

    <EmptyState v-if="projects.length === 0" title="No projects yet" icon="folder">
      Add a project folder. Kova detects how to run it.
      <template #actions>
        <UiButton variant="primary" size="md" @click="handleAdd">Add project</UiButton>
      </template>
    </EmptyState>

    <div v-else class="pp-list">
      <div
        v-for="p in projects"
        :key="p.id"
        class="pp-card"
        :class="{ 'pp-card--selected': selectedId === p.id }"
        @click="emit('select', p.id)"
      >
        <div class="pp-row">
          <div class="pp-who">
            <div class="pp-name">
              <!-- Tombol, supaya project bisa dipilih dengan keyboard; klik
                   di area kartu lainnya tetap memilih lewat @click kartu. -->
              <button
                type="button"
                class="pp-name__btn"
                :aria-current="selectedId === p.id ? 'true' : undefined"
                @click.stop="emit('select', p.id)"
              >{{ p.name }}</button>
              <UiChip v-if="p.kind">{{ p.kind }}</UiChip>
              <UiChip v-if="p.autoStart">auto-start</UiChip>
              <UiChip v-if="p.startServices && p.profileId">+ services</UiChip>
            </div>
            <div class="pp-path" :title="p.path">{{ p.path || 'Folder not chosen yet (from an old .test site)' }}</div>
          </div>

          <template v-if="canRun(p)">
            <button
              v-if="stateOf(p.id) === 'running' || stateOf(p.id) === 'unresponsive'"
              class="pp-url"
              :title="`Open ${url(p)}`"
              @click.stop="handleOpen(p)"
            >localhost:{{ p.port }}</button>
            <span v-else class="pp-url pp-url--off">localhost:{{ p.port }}</span>
            <StatusDot class="pp-state" :state="dotState(p)" :label="stateLabel(p)" />
          </template>

          <div class="pp-actions" @click.stop>
            <UiButton variant="accent" v-if="!p.path" @click="handleChooseFolder(p)">Choose folder</UiButton>
            <template v-else-if="canRun(p)">
              <UiButton
                v-if="isActive(stateOf(p.id))"
                :disabled="busy[p.id]"
                @click="handleStop(p)"
              >Stop</UiButton>
              <UiButton variant="accent" v-else :loading="busy[p.id]" @click="handleStart(p)">
                Start
              </UiButton>
              <UiButton
                :disabled="busy[p.id] || !isActive(stateOf(p.id))"
                :title="isActive(stateOf(p.id)) ? 'Stop, then start again' : 'Project is not running'"
                @click="handleRestart(p)"
              >Restart</UiButton>
            </template>
            <UiMenu :items="menuItems(p)" :label="`More actions for ${p.name}`" @select="onMenu(p, $event)" />
          </div>
        </div>

        <p v-if="!canRun(p)" class="pp-note">
          {{ missing(p) }}
          <button v-if="p.path" class="pp-link" @click.stop="startEdit(p)">Project settings</button>
        </p>
        <UiBanner v-if="errors[p.id]" tone="err" title="That did not work">{{ errors[p.id] }}</UiBanner>
        <UiBanner
          v-else-if="infoOf(p.id)?.state === 'exited' && infoOf(p.id)?.lastLine"
          tone="err"
          :title="infoOf(p.id)?.exitCode != null ? `Stopped with exit code ${infoOf(p.id)?.exitCode}` : 'Stopped'"
        >
          <code class="pp-lastline">{{ infoOf(p.id)?.lastLine }}</code>
          <template #actions><UiButton @click.stop="viewLog(p)">View log</UiButton></template>
        </UiBanner>
        <UiBanner
          v-else-if="stateOf(p.id) === 'unresponsive'"
          tone="warn"
          :title="`Running, but nothing answers on :${p.port}`"
        >
          The app may be listening on a different port. Check the log, then set that port in Project settings.
          <template #actions>
            <UiButton @click.stop="viewLog(p)">View log</UiButton>
            <UiButton @click.stop="startEdit(p)">Settings</UiButton>
          </template>
        </UiBanner>
      </div>
    </div>

    <UiBanner v-if="panelError" tone="err" title="Could not add the project">{{ panelError }}</UiBanner>
    <div v-if="projects.length > 0"><UiButton variant="accent" @click="handleAdd">+ Add project folder</UiButton></div>

    <UiDrawer :open="!!editing" :title="editing ? `Project settings · ${editing.name}` : 'Project settings'" @close="cancelEdit">
      <template v-if="editing">
        <UiInput v-model="form.name" label="Name" />
        <UiInput v-model="editingPath" label="Folder" mono readonly />
        <div class="pp-cmd">
          <UiInput
            v-model="form.command"
            label="Command"
            mono
            placeholder="e.g. npm run dev -- --port {port}"
            help="{port} is replaced by the port, and the PORT variable is set to it. Leave empty to use this project only for versions and services."
          />
          <UiButton class="pp-cmd__detect" @click="redetect(editing)">Detect</UiButton>
        </div>
        <UiInput v-model="form.port" label="Port" mono placeholder="8000" width="120px" />
        <UiInput v-model="form.profileId" label="Services" as="select" :options="profileOptions" />
        <label class="pp-check" :class="{ 'pp-check--off': !form.profileId }">
          <input v-model="form.startServices" type="checkbox" :disabled="!form.profileId" />
          Start these services when the project starts
        </label>
        <UiInput
          v-model="form.env"
          label="Environment"
          as="textarea"
          mono
          :rows="4"
          placeholder="SSL_CERT_FILE=C:\ssl\ca-bundle.crt&#10;NODE_OPTIONS=--max-old-space-size=4096"
          help="One KEY=VALUE per line. Overrides values from mise, for the dev server Kova starts only."
        />
        <label class="pp-check">
          <input v-model="form.autoStart" type="checkbox" />
          Start this project when Kova opens
        </label>
        <p class="pp-form__hint">
          Starting services leaves other running services alone and never stops them when the project stops.
          Changes apply the next time the project starts.
        </p>
        <UiBanner v-if="formError" tone="err" title="Check the form">{{ formError }}</UiBanner>
      </template>
      <template v-if="editing" #footer>
        <UiButton
          variant="primary"
          size="md"
          v-if="!isActive(stateOf(editing.id))"
          @click="saveEdit(editing, true)"
        >Save &amp; start</UiButton>
        <UiButton size="md" @click="saveEdit(editing)">Save</UiButton>
        <UiButton variant="quiet" size="md" @click="cancelEdit">Cancel</UiButton>
      </template>
    </UiDrawer>

    <UiModal
      :open="!!removing"
      :title="removing ? `Remove ${removing.name} from Kova?` : ''"
      size="sm"
      danger
      @close="removing = null"
    >
      <p class="pp-modal-text">
        The project leaves the list and its dev server is stopped. The folder, its files and mise.toml are not touched.
      </p>
      <template #footer>
        <UiButton size="md" data-autofocus @click="removing = null">Cancel</UiButton>
        <UiButton variant="danger" size="md" :loading="removeBusy" @click="confirmRemove">Remove project</UiButton>
      </template>
    </UiModal>
  </div>
</template>

<style scoped>
.pp-lastline { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--red); }
.pp-cmd { display: flex; align-items: flex-start; gap: var(--space-2); }
.pp-cmd > :first-child { flex: 1; }
.pp-cmd__detect { margin-top: 22px; height: 32px; }
.pp-modal-text { margin: 0; }

.pp-link {
  margin-left: var(--space-2);
  padding: 0;
  background: none;
  border: 0;
  font: inherit;
  font-family: var(--font-sans, inherit);
  color: var(--accent);
  text-decoration: underline;
  cursor: pointer;
}
.pp-link:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

.pp {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.pp-title {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--faint);
}

.pp-hint,
.pp-note {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  line-height: 1.55;
  color: var(--muted);
  margin: 0;
}

.pp-note {
  color: var(--amber);
}

.pp-error {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--red);
  margin: 0;
  overflow-wrap: anywhere;
}

.pp-error--mono {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
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

.pp-card { cursor: pointer; transition: border-color var(--dur-fast) var(--ease); }
.pp-card:hover { border-color: var(--border-strong); }
.pp-card--selected { border-color: color-mix(in srgb, var(--accent) 55%, transparent); }

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

.pp-name__btn {
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
  border-radius: var(--radius-xs);
}
.pp-name__btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.pp-name {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text);
}

.pp-path {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pp-url {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
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
  color: var(--faint);
  text-decoration: none;
  cursor: default;
}

.pp-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.pp-url:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.pp-check--off {
  opacity: 0.5;
}

.pp-check {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--text);
}
.pp-check input { width: 16px; height: 16px; margin: 0; accent-color: var(--accent); }
.pp-state { flex: none; }

.pp-form__hint {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.55;
  color: var(--faint);
}

.pp-form__hint code {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--muted);
}

</style>
