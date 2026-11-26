<script setup lang="ts">
// Isi form tambah project: deteksi jenis folder, cek port, lalu simpan.
// Dipakai modal Add project dan langkah "First project" di onboarding;
// tombolnya milik pemanggil (save() diekspos).
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiInput from '@/components/ui/UiInput.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'
import CommandSuggestions from '@/components/CommandSuggestions.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import FrameworkLogo from '@/components/ui/FrameworkLogo.vue'
import { frameworkColor, frameworkIcon } from '@/utils/framework'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime, type ProjectSuggestion } from '@/composables/useProjectRuntime'
import { useProjectActions } from '@/composables/useProjectActions'
import { useVersionContext } from '@/composables/useVersionContext'
import type { Project, RunMode, ToolCurrent } from '@/types/mise'
import { wslDistro } from '@/utils/wsl'
import { samePath } from '@/utils/paths'
import { containerPort, projectPort } from '@/utils/ports'

const props = withDefaults(defineProps<{ path: string; compact?: boolean }>(), { compact: false })

const { config, addProject } = useConfig()
const { detect } = useProjectRuntime()
const actions = useProjectActions()
const versionCtx = useVersionContext()

const detecting = ref(true)
const detectError = ref('')
const suggestion = ref<ProjectSuggestion | null>(null)
const tools = ref<ToolCurrent[]>([])
const form = ref({ name: '', port: '', command: '', profileId: '', startServices: false, autoStart: false })
/** Folder di distro WSL: default jalan di dalam distro. */
const distro = computed(() => wslDistro(props.path))
const runInWsl = ref(true)
const portState = ref<{ free: boolean; reason: string; suggestion: number | null } | null>(null)
const formError = ref('')
const saving = ref(false)
const portChecking = ref(false)

// Run with (canvas K1): ditawarkan hanya bila folder punya Dockerfile atau compose.
const cinfo = ref<{ dockerfile: boolean; composeFile: string | null; composeServices: string[]; exposePort: number | null } | null>(null)
const runMode = ref<RunMode>('command')
const containerPortText = ref('')
const runModes = computed(() => [
  { value: 'command', label: 'Command' },
  ...(cinfo.value?.dockerfile ? [{ value: 'dockerfile', label: 'Dockerfile' }] : []),
  ...(cinfo.value?.composeFile ? [{ value: 'compose', label: 'Compose' }] : []),
])
const foundFiles = computed(() => [cinfo.value?.dockerfile ? 'Dockerfile' : '', cinfo.value?.composeFile ?? ''].filter(Boolean))

const known = computed(() => !!suggestion.value && suggestion.value.detection.kind !== 'unknown')
const logo = computed(() => frameworkIcon(suggestion.value?.detection.label))
const why = computed(() => {
  const d = suggestion.value
  if (!d) return ''
  return [d.detection.reason || d.evidence.join(', '), runtimeText.value].filter(Boolean).join(' · ')
})

const runtimeText = computed(() =>
  tools.value
    .filter((t) => t.version)
    .map((t) => `${t.tool.toUpperCase() === 'PHP' ? 'PHP' : t.tool[0].toUpperCase() + t.tool.slice(1)} ${t.version} from ${t.scope === 'global' ? 'global' : (t.source?.split(/[\\/]/).pop() ?? 'mise')}`)
    .join(' · '),
)

const profileOptions = computed(() => [
  { value: '', label: 'No service profile' },
  ...config.value.profiles.map((pr) => ({ value: pr.id, label: pr.name })),
])

async function run(): Promise<void> {
  detecting.value = true
  detectError.value = ''
  suggestion.value = null
  try {
    const s = await detect(props.path, actions.takenPorts())
    suggestion.value = s
    form.value = { ...form.value, name: s.name, port: String(s.port), command: s.detection.command }
    portState.value = { free: true, reason: '', suggestion: null }
    runMode.value = 'command'
    try {
      cinfo.value = await invoke('project_container_info', { path: props.path })
      containerPortText.value = cinfo.value?.exposePort ? String(cinfo.value.exposePort) : ''
    } catch {
      cinfo.value = null
    }
    try {
      // Folder WSL: versi dari distro (yang dipakai bila Run inside WSL menyala).
      tools.value = await invoke<ToolCurrent[]>(distro.value ? 'wsl_project_tools' : 'mise_project_tools', { path: props.path })
    } catch {
      tools.value = []
    }
  } catch (err) {
    detectError.value = String(err)
  } finally {
    detecting.value = false
  }
}
watch(() => props.path, run, { immediate: true })

let portTimer: ReturnType<typeof setTimeout> | undefined
watch(
  () => form.value.port,
  (raw) => {
    clearTimeout(portTimer)
    if (detecting.value) return
    const port = projectPort(raw)
    if (port === null) {
      portState.value = { free: false, reason: 'Use a port from 1024 to 65535.', suggestion: null }
      return
    }
    // Hasil port sebelumnya tidak berlaku lagi sampai cek untuk nilai ini kembali.
    portChecking.value = true
    portTimer = setTimeout(async () => {
      try {
        const r = await invoke<{ free: boolean; reason: string; suggestion: number | null }>('project_port_check', { port, taken: actions.takenPorts() })
        if (form.value.port === raw) portState.value = r
      } catch {
        if (form.value.port === raw) portState.value = null
      } finally {
        if (form.value.port === raw) portChecking.value = false
      }
    }, 250)
  },
)

const portHelp = computed(() => {
  if (!portState.value) return ''
  return portState.value.free ? `${form.value.port} is free` : ''
})

/** Simpan project baru. Mengembalikan project (atau null bila form belum benar). */
async function save(start = false): Promise<Project | null> {
  formError.value = ''
  const name = form.value.name.trim()
  if (!name) {
    formError.value = 'Name is required.'
    return null
  }
  const port = projectPort(form.value.port)
  if (port === null) {
    formError.value = 'Use a port from 1024 to 65535.'
    return null
  }
  if (portChecking.value) {
    formError.value = `Still checking port ${port}. Try again in a moment.`
    return null
  }
  if (portState.value && !portState.value.free) {
    formError.value = portState.value.reason
    return null
  }
  const cport = containerPort(containerPortText.value)
  if (runMode.value === 'dockerfile' && cport === null) {
    formError.value = 'Port in the container must be 1 to 65535.'
    return null
  }
  if (config.value.projects.some((p) => samePath(p.path, props.path))) {
    formError.value = 'This folder is already in the list.'
    return null
  }
  saving.value = true
  try {
    const project = await addProject(props.path, {
      name,
      kind: suggestion.value?.detection.label || undefined,
      command: form.value.command.trim(),
      port,
      profileId: form.value.profileId || null,
      startServices: form.value.profileId ? form.value.startServices : false,
      autoStart: form.value.autoStart,
      wsl: !!distro.value && runInWsl.value,
      runMode: runMode.value,
      container:
        runMode.value === 'command'
          ? null
          : { dockerfile: '', containerPort: cport ?? 0, joinServices: true, composeFile: cinfo.value?.composeFile ?? '' },
    })
    await versionCtx.refresh()
    if (project && start && (project.command?.trim() || runMode.value !== 'command')) await actions.start(project)
    return project
  } catch (err) {
    formError.value = String(err)
    return null
  } finally {
    saving.value = false
  }
}

const ready = computed(() => !detecting.value && !detectError.value)
defineExpose({ save, ready, saving, canStart: computed(() => runMode.value !== 'command' || !!form.value.command.trim()) })
</script>

<template>
  <div class="apf">
    <div class="apf-path" :title="path">{{ path }}</div>

    <div v-if="detecting" class="apf-detecting" aria-busy="true">
      <span class="apf-spinner" aria-hidden="true"></span>
      Detecting project type…
      <SkeletonBox width="100%" height="32px" />
      <SkeletonBox width="60%" height="32px" />
    </div>

    <UiBanner v-else-if="detectError" tone="err" title="Could not read this folder">{{ detectError }}</UiBanner>

    <template v-else>
      <div v-if="known && logo" class="apf-det" role="status" :style="{ '--fw': frameworkColor(logo) }">
        <span class="apf-det__tile"><FrameworkLogo :kind="suggestion?.detection.label" :size="16" /></span>
        <span class="apf-det__body">
          <span class="apf-det__title">Detected {{ suggestion?.detection.label }}</span>
          <span v-if="why" class="apf-det__why">{{ why }}</span>
        </span>
      </div>
      <UiBanner v-else-if="known" tone="ok" :title="`Detected ${suggestion?.detection.label}`">{{ why }}</UiBanner>
      <UiBanner v-else tone="warn" title="Couldn't tell how to run this folder">
        No package.json, composer.json, go.mod, artisan or Python project file found. Enter the command yourself.
      </UiBanner>

      <div v-if="foundFiles.length" class="apf-run">
        <span class="apf-run__label">Run with</span>
        <UiSegmented v-model="runMode" :options="runModes" label="Run with" />
        <span class="apf-run__found">Found in the folder: <code v-for="f in foundFiles" :key="f">{{ f }}</code></span>
      </div>

      <div class="apf-row">
        <UiInput v-model="form.name" label="Name" />
        <UiInput v-model="form.port" label="Port" mono width="120px" :help="portHelp" :error="portState && !portState.free ? portState.reason : ''" />
      </div>
      <div v-if="portState && !portState.free && portState.suggestion" class="apf-use">
        <UiButton @click="form.port = String(portState.suggestion)">Use {{ portState.suggestion }}</UiButton>
      </div>
      <template v-if="runMode === 'command'">
        <UiInput
          v-model="form.command"
          label="Command"
          mono
          placeholder="e.g. npm run dev -- --port {port}"
          help="{port} is replaced by the port above."
        />
        <CommandSuggestions v-model="form.command" :path="props.path" />
      </template>
      <UiInput
        v-else-if="runMode === 'dockerfile'"
        v-model="containerPortText"
        label="Port in the container"
        mono
        width="260px"
        :placeholder="form.port"
        :help="cinfo?.exposePort && containerPortText === String(cinfo.exposePort) ? 'From EXPOSE in the Dockerfile' : 'Empty = same as Port above'"
      />
      <p v-else class="apf-run__hint">Kova runs docker compose up for this folder. Stop runs compose down. More options are in Project settings.</p>

      <template v-if="!compact">
        <UiInput v-model="form.profileId" label="Service profile" as="select" :options="profileOptions" />
        <div class="apf-toggles">
          <label class="apf-toggle" :class="{ 'apf-toggle--off': !form.profileId }">
            <UiToggle
              size="sm"
              :on="form.startServices && !!form.profileId"
              :disabled="!form.profileId"
              label="Start its services with the project"
              @click="form.startServices = !form.startServices"
            />
            Start its services with the project
          </label>
          <label class="apf-toggle">
            <UiToggle size="sm" :on="form.autoStart" label="Start when Kova opens" @click="form.autoStart = !form.autoStart" />
            Start when Kova opens
          </label>
          <label v-if="distro" class="apf-toggle">
            <UiToggle size="sm" :on="runInWsl" :label="`Run inside WSL (${distro})`" @click="runInWsl = !runInWsl" />
            Run inside WSL ({{ distro }})
          </label>
        </div>
      </template>

      <UiBanner v-if="formError" tone="err" title="Check the form">{{ formError }}</UiBanner>
    </template>
  </div>
</template>

<style scoped>
.apf { display: flex; flex-direction: column; gap: var(--space-3); }
.apf-path { font: 400 var(--text-xs) var(--font-mono); color: var(--muted); overflow-wrap: anywhere; }
.apf-detecting { display: flex; flex-direction: column; gap: var(--space-2); font-size: var(--text-sm); color: var(--muted); }
.apf-spinner {
  display: inline-block;
  width: 12px;
  height: 12px;
  margin-right: var(--space-2);
  border: 2px solid var(--border-strong);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: apf-spin 0.8s linear infinite;
  vertical-align: -1px;
}
@keyframes apf-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .apf-spinner { animation: none; } }
.apf-det {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 10px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface2);
  font-size: var(--text-sm);
}
.apf-det__tile {
  flex: none;
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-xs);
  border: 1px solid color-mix(in srgb, var(--fw) 45%, transparent);
  background: color-mix(in srgb, var(--fw) 12%, transparent);
}
.apf-det__body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.apf-det__title { font-weight: 600; color: var(--text); }
.apf-det__why { color: var(--muted); overflow-wrap: anywhere; }
.apf-row { display: flex; gap: var(--space-3); align-items: flex-start; }
.apf-row > :first-child { flex: 1; }
.apf-use { margin-top: calc(-1 * var(--space-2)); }
.apf-run { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2) var(--space-3); }
.apf-run__label { font-size: var(--text-xs); font-weight: 500; color: var(--muted); width: 100%; }
.apf-run__found, .apf-run__hint { font-size: var(--text-xs); color: var(--muted); margin: 0; }
.apf-run__found code { font-family: var(--font-mono); color: var(--text); margin-left: var(--space-2); }
.apf-toggles { display: flex; flex-wrap: wrap; gap: var(--space-4); }
.apf-toggle { display: inline-flex; align-items: center; gap: var(--space-2); font-size: var(--text-sm); color: var(--text); }
.apf-toggle--off { color: var(--faint); }
</style>
