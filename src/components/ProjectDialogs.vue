<script setup lang="ts">
// Drawer "Project settings" dan dialog hapus. Dibuka dari menu ⋯ atau dari
// banner di panel detail lewat useProjectActions.
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import UiDrawer from '@/components/ui/UiDrawer.vue'
import UiModal from '@/components/ui/UiModal.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiButton from '@/components/ui/UiButton.vue'
import CommandSuggestions from '@/components/CommandSuggestions.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import { wslDistro } from '@/utils/wsl'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import { useEngineStatus } from '@/composables/useEngineStatus'
import { useServices } from '@/composables/useServices'
import type { RunMode } from '@/types/mise'
import FrameworkChip from '@/components/FrameworkChip.vue'
import { useConfig } from '@/composables/useConfig'
import { useVersionContext } from '@/composables/useVersionContext'
import { useProjectRuntime, isActive } from '@/composables/useProjectRuntime'
import { useProjectActions } from '@/composables/useProjectActions'

const { config, updateProject, removeProject } = useConfig()
const versionCtx = useVersionContext()
const runtime = useProjectRuntime()
const actions = useProjectActions()
const { editingId, removingId } = actions

const editing = computed(() => config.value.projects.find((p) => p.id === editingId.value) ?? null)
const removing = computed(() => config.value.projects.find((p) => p.id === removingId.value) ?? null)

const form = ref({ name: '', path: '', command: '', port: '', profileId: '', startServices: false, autoStart: false, wsl: false })
// Run with (Flow K): Command, Dockerfile, atau Compose.
const runMode = ref<RunMode>('command')
const cform = ref({ dockerfile: '', containerPort: '', joinServices: true, composeFile: '', skip: [] as string[] })
const cinfo = ref<{ dockerfile: boolean; composeFile: string | null; composeServices: string[] } | null>(null)
const { engine } = useEngineStatus()
const svc = useServices()
const runModes = [
  { value: 'command', label: 'Command' },
  { value: 'dockerfile', label: 'Dockerfile' },
  { value: 'compose', label: 'Compose' },
]
/** Service compose yang namanya sama dengan service Kova: tawarkan pakai milik Kova. */
const overlapping = computed(() => (cinfo.value?.composeServices ?? []).filter((n) => svc.definitions.value.some((d) => d.id === n || d.name.toLowerCase() === n.toLowerCase())))
async function loadContainerInfo(): Promise<void> {
  cinfo.value = null
  if (!form.value.path) return
  try {
    cinfo.value = await invoke('project_container_info', { path: form.value.path })
  } catch {
    cinfo.value = null
  }
}
watch(() => form.value.path, loadContainerInfo)
const formDistro = computed(() => wslDistro(form.value.path))
const formError = ref('')
const portState = ref<{ free: boolean; reason: string; suggestion: number | null } | null>(null)
const saving = ref(false)
/** Hasil tombol Detect: jenis baru ikut disimpan, supaya logo dan label terbarui. */
const detected = ref<{ label: string; reason: string } | null>(null)

watch(editing, (p) => {
  if (!p) return
  formError.value = ''
  portState.value = null
  detected.value = null
  form.value = {
    name: p.name,
    path: p.path,
    command: p.command ?? '',
    port: (p.port ?? 0) > 0 ? String(p.port) : '',
    profileId: p.profileId ?? '',
    startServices: p.startServices ?? false,
    autoStart: p.autoStart ?? false,
    wsl: p.wsl ?? false,
  }
  runMode.value = p.runMode ?? 'command'
  const c = p.container ?? {}
  cform.value = {
    dockerfile: c.dockerfile ?? '',
    containerPort: c.containerPort ? String(c.containerPort) : '',
    joinServices: c.joinServices ?? true,
    composeFile: c.composeFile ?? '',
    skip: [...(c.skipComposeServices ?? [])],
  }
  void loadContainerInfo()
})

let timer: ReturnType<typeof setTimeout> | undefined
watch(
  () => form.value.port,
  (raw) => {
    clearTimeout(timer)
    const p = editing.value
    if (!p) return
    if (raw.trim() === '') {
      portState.value = null
      return
    }
    const port = Number(raw)
    if (!/^\d{1,5}$/.test(raw.trim()) || port > 65535) {
      portState.value = { free: false, reason: 'Use a port from 1024 to 65535.', suggestion: null }
      return
    }
    timer = setTimeout(async () => {
      try {
        portState.value = await invoke('project_port_check', {
          port,
          taken: actions.takenPorts(p.id),
          own: port === p.port && isActive(runtime.stateOf(p.id)),
        })
      } catch {
        portState.value = null
      }
    }, 250)
  },
)

const profileOptions = computed(() => [
  { value: '', label: 'No service profile' },
  ...config.value.profiles.map((pr) => ({ value: pr.id, label: pr.name })),
])

async function changeFolder(): Promise<void> {
  const picked = await openDialog({ directory: true, multiple: false, title: 'Choose the project folder' })
  if (typeof picked === 'string') form.value.path = picked.replace(/[\\/]+$/, '')
}

async function redetect(): Promise<void> {
  const p = editing.value
  if (!p || !form.value.path) return
  try {
    const s = await runtime.detect(form.value.path, actions.takenPorts(p.id))
    form.value.command = s.detection.command
    detected.value = s.detection.label ? { label: s.detection.label, reason: s.detection.reason || s.evidence.join(', ') } : null
    if (!form.value.port) form.value.port = String(s.port)
    formError.value = s.detection.command ? '' : 'Project type not recognized. Type the command yourself.'
  } catch (err) {
    formError.value = String(err)
  }
}

async function save(): Promise<void> {
  const p = editing.value
  if (!p) return
  const name = form.value.name.trim()
  if (!name) return void (formError.value = 'Name is required.')
  if (portState.value && !portState.value.free) return void (formError.value = portState.value.reason)
  saving.value = true
  try {
    const pathChanged = form.value.path !== p.path
    let kind = detected.value?.label ?? p.kind
    if (pathChanged && form.value.path && !detected.value) {
      kind = (await runtime.detect(form.value.path, actions.takenPorts(p.id))).detection.label
    }
    await updateProject(p.id, {
      name,
      path: form.value.path,
      kind,
      command: form.value.command.trim(),
      port: form.value.port.trim() === '' ? 0 : Number(form.value.port),
      profileId: form.value.profileId || null,
      startServices: form.value.profileId ? form.value.startServices : false,
      autoStart: form.value.autoStart,
      wsl: formDistro.value ? form.value.wsl : false,
      runMode: runMode.value,
      container: runMode.value === 'command' ? null : {
        dockerfile: cform.value.dockerfile.trim(),
        containerPort: Number(cform.value.containerPort) || 0,
        joinServices: cform.value.joinServices,
        composeFile: cform.value.composeFile.trim(),
        skipComposeServices: cform.value.skip,
      },
    })
    if (pathChanged) await versionCtx.refresh()
    editingId.value = null
  } catch (err) {
    formError.value = String(err)
  } finally {
    saving.value = false
  }
}

const removeBusy = ref(false)
async function confirmRemove(): Promise<void> {
  const p = removing.value
  if (!p) return
  removeBusy.value = true
  try {
    if (isActive(runtime.stateOf(p.id))) await runtime.stop(p.id)
    await removeProject(p.id)
    await runtime.forget(p.id)
    await versionCtx.refresh()
    if (editingId.value === p.id) editingId.value = null
  } catch (err) {
    actions.setError(p.id, String(err))
  } finally {
    removeBusy.value = false
    removingId.value = null
  }
}
</script>

<template>
  <UiDrawer :open="!!editing" title="Project settings" @close="editingId = null">
    <template v-if="editing">
      <UiInput v-model="form.name" label="Name" />
      <div class="pdl-row">
        <UiInput v-model="form.path" label="Folder" mono readonly aria-label="Folder" />
        <UiButton class="pdl-row__btn" @click="changeFolder">Change</UiButton>
      </div>
      <div class="pdl-run">
        <span class="pdl-label">Run with</span>
        <UiSegmented v-model="runMode" :options="runModes" label="Run with" />
        <p class="pdl-hint">{{ runMode === 'command'
          ? 'Kova runs the command in the project folder.'
          : runMode === 'dockerfile'
            ? 'Kova builds the image and runs it as a container next to the services. Environment tab values are passed with -e.'
            : 'Kova runs docker compose up for this folder. Stop runs compose down.' }}</p>
      </div>
      <template v-if="runMode === 'dockerfile'">
        <div class="pdl-grid2">
          <UiInput v-model="cform.dockerfile" label="Dockerfile" mono placeholder="Dockerfile" :help="cinfo?.dockerfile || cform.dockerfile ? '' : 'No Dockerfile found in the folder'" />
          <UiInput v-model="cform.containerPort" label="Port in the container" mono :placeholder="form.port || '8080'" help="Empty = same as Port below" />
        </div>
      </template>
      <template v-if="runMode === 'compose'">
        <UiBanner v-if="engine === 'wslc'" tone="warn" title="Compose needs the Docker engine">WSL containers has no compose yet. Switch the engine in Settings › Services, or use Dockerfile.</UiBanner>
        <UiInput v-model="cform.composeFile" label="Compose file" mono :placeholder="cinfo?.composeFile ?? 'compose.yaml'" :help="cinfo?.composeServices.length ? `Found: ${cinfo.composeServices.join(', ')}` : 'No compose file found in the folder'" />
        <div v-if="overlapping.length" class="pdl-overlap">
          <span class="pdl-label">Also a Kova service</span>
          <label v-for="n in overlapping" :key="n" class="pdl-toggle">
            <UiToggle size="sm" :on="cform.skip.includes(n)" :label="`Use Kova's ${n}`" @click="cform.skip = cform.skip.includes(n) ? cform.skip.filter((x) => x !== n) : [...cform.skip, n]" />
            Use Kova's {{ n }} (shared data) instead of the compose one
          </label>
        </div>
      </template>
      <label v-if="runMode !== 'command'" class="pdl-toggle" :class="{ 'pdl-toggle--off': engine === 'wslc' }">
        <UiToggle size="sm" :on="cform.joinServices && engine !== 'wslc'" :disabled="engine === 'wslc'" label="Join the services network" @click="cform.joinServices = !cform.joinServices" />
        Join the services network (reach MySQL as servel_mysql:3306)
      </label>
      <p v-if="runMode !== 'command' && engine === 'wslc'" class="pdl-hint">WSL containers has no shared network; the container reaches services through their ports on this PC.</p>
      <div v-if="runMode === 'command'" class="pdl-row">
        <UiInput
          v-model="form.command"
          label="Command"
          mono
          placeholder="e.g. npm run dev -- --port {port}"
          help="{port} is replaced by the port, and PORT is set to it. Empty = use the project only for versions and services."
        />
        <UiButton class="pdl-row__btn" @click="redetect">Detect</UiButton>
      </div>
      <p v-if="detected" class="pdl-det" role="status">
        <FrameworkChip :kind="detected.label" />
        <span>{{ detected.reason }}</span>
      </p>
      <CommandSuggestions v-if="runMode === 'command'" v-model="form.command" :path="form.path" />
      <UiInput
        v-model="form.port"
        label="Port"
        mono
        width="140px"
        placeholder="8000"
        :help="portState?.free ? `${form.port} is free` : ''"
        :error="portState && !portState.free ? portState.reason : ''"
      />
      <div v-if="portState && !portState.free && portState.suggestion">
        <UiButton @click="form.port = String(portState.suggestion)">Use {{ portState.suggestion }}</UiButton>
      </div>
      <UiInput v-model="form.profileId" label="Services" as="select" :options="profileOptions" aria-label="Service profile" />
      <label class="pdl-toggle" :class="{ 'pdl-toggle--off': !form.profileId }">
        <UiToggle
          size="sm"
          :on="form.startServices && !!form.profileId"
          :disabled="!form.profileId"
          label="Start services with project"
          @click="form.startServices = !form.startServices"
        />
        Start services with project
      </label>
      <label class="pdl-toggle">
        <UiToggle size="sm" :on="form.autoStart" label="Start when Kova opens" @click="form.autoStart = !form.autoStart" />
        Start when Kova opens
      </label>
      <template v-if="formDistro && runMode === 'command'">
        <label class="pdl-toggle">
          <UiToggle size="sm" :on="form.wsl" :label="`Run inside WSL (${formDistro})`" @click="form.wsl = !form.wsl" />
          Run inside WSL ({{ formDistro }})
        </label>
        <p class="pdl-hint">{{ form.wsl
          ? `The command runs in ${formDistro} with its own tools (Go, PHP, Node, mise there). Environment variables and PORT are passed in. Turn off to run Windows programs such as wslc.`
          : `The command runs with Windows programs on the \\\\wsl.localhost folder. Turn on to use the tools inside ${formDistro}.` }}</p>
      </template>
      <p class="pdl-hint">Environment variables are on the project's Environment tab. Changes apply the next time the project starts.</p>
      <UiBanner v-if="formError" tone="err" title="Check the form">{{ formError }}</UiBanner>
    </template>
    <template v-if="editing" #footer>
      <UiButton variant="quiet" size="md" @click="editingId = null">Cancel</UiButton>
      <UiButton variant="primary" size="md" :loading="saving" @click="save">Save</UiButton>
    </template>
  </UiDrawer>

  <UiModal :open="!!removing" :title="removing ? `Remove ${removing.name}?` : ''" size="sm" danger @close="removingId = null">
    <p class="pdl-text">
      Kova stops its dev server and forgets its settings. The folder
      <code>{{ removing?.path }}</code>
      and its files stay on disk.
    </p>
    <template #footer>
      <UiButton size="md" data-autofocus @click="removingId = null">Cancel</UiButton>
      <UiButton variant="danger" size="md" :loading="removeBusy" @click="confirmRemove">Remove project</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.pdl-row { display: flex; align-items: flex-start; gap: var(--space-2); }
.pdl-det { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); margin: calc(-1 * var(--space-1)) 0 0; font-size: var(--text-xs); color: var(--muted); }
.pdl-row > :first-child { flex: 1; }
.pdl-row__btn { margin-top: 22px; height: 32px; }
.pdl-toggle { display: flex; align-items: center; gap: var(--space-2); font-size: var(--text-sm); color: var(--text); }
.pdl-toggle--off { color: var(--faint); }
.pdl-hint { margin: 0; font-size: var(--text-xs); color: var(--faint); }
.pdl-text { margin: 0; line-height: 1.6; }
.pdl-text code { font: 400 var(--text-xs) var(--font-mono); color: var(--muted); overflow-wrap: anywhere; }
.pdl-run { display: flex; flex-direction: column; gap: var(--space-2); }
.pdl-label { font-size: var(--text-xs); font-weight: 500; color: var(--muted); }
.pdl-grid2 { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-3); }
.pdl-overlap { display: flex; flex-direction: column; gap: var(--space-2); }
</style>
