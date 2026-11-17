<script setup lang="ts">
// Isi form tambah project (v3.4, canvas B3–B4 dan onboarding A6–A7): deteksi
// jenis folder, cek port, lalu simpan. Dipakai modal Add project dan langkah
// "First project" di onboarding; tombolnya milik pemanggil (save() diekspos).
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiInput from '@/components/ui/UiInput.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime, type ProjectSuggestion } from '@/composables/useProjectRuntime'
import { useProjectActions } from '@/composables/useProjectActions'
import { useVersionContext } from '@/composables/useVersionContext'
import type { Project, ToolCurrent } from '@/types/mise'

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
const portState = ref<{ free: boolean; reason: string; suggestion: number | null } | null>(null)
const formError = ref('')
const saving = ref(false)

const known = computed(() => !!suggestion.value && suggestion.value.detection.kind !== 'unknown')

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
    try {
      tools.value = await invoke<ToolCurrent[]>('mise_project_tools', { path: props.path })
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
    const port = Number(raw)
    if (!/^\d{1,5}$/.test(raw.trim()) || port > 65535) {
      portState.value = { free: false, reason: 'Use a port from 1024 to 65535.', suggestion: null }
      return
    }
    portTimer = setTimeout(async () => {
      try {
        portState.value = await invoke('project_port_check', { port, taken: actions.takenPorts() })
      } catch {
        portState.value = null
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
  if (portState.value && !portState.value.free) {
    formError.value = portState.value.reason
    return null
  }
  const port = Number(form.value.port)
  if (config.value.projects.some((p) => p.path.toLowerCase() === props.path.replace(/[\\/]+$/, '').toLowerCase())) {
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
    })
    await versionCtx.refresh()
    if (project && start && project.command?.trim()) await actions.start(project)
    return project
  } catch (err) {
    formError.value = String(err)
    return null
  } finally {
    saving.value = false
  }
}

const ready = computed(() => !detecting.value && !detectError.value)
defineExpose({ save, ready, saving, canStart: computed(() => !!form.value.command.trim()) })
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
      <UiBanner v-if="known" tone="ok" :title="`Detected ${suggestion?.detection.label}`">
        {{ [suggestion?.evidence.join(', '), runtimeText].filter(Boolean).join(' · ') }}
      </UiBanner>
      <UiBanner v-else tone="warn" title="Couldn't tell how to run this folder">
        No package.json, composer.json, go.mod, artisan or Python project file found. Enter the command yourself.
      </UiBanner>

      <div class="apf-row">
        <UiInput v-model="form.name" label="Name" />
        <UiInput v-model="form.port" label="Port" mono width="120px" :help="portHelp" :error="portState && !portState.free ? portState.reason : ''" />
      </div>
      <div v-if="portState && !portState.free && portState.suggestion" class="apf-use">
        <UiButton @click="form.port = String(portState.suggestion)">Use {{ portState.suggestion }}</UiButton>
      </div>
      <UiInput
        v-model="form.command"
        label="Command"
        mono
        placeholder="e.g. npm run dev -- --port {port}"
        help="{port} is replaced by the port above."
      />

      <template v-if="!compact">
        <UiInput v-model="form.profileId" label="Services" as="select" :options="profileOptions" />
        <div class="apf-toggles">
          <label class="apf-toggle" :class="{ 'apf-toggle--off': !form.profileId }">
            <UiToggle
              size="sm"
              :on="form.startServices && !!form.profileId"
              :disabled="!form.profileId"
              label="Start services with project"
              @click="form.startServices = !form.startServices"
            />
            Start with project
          </label>
          <label class="apf-toggle">
            <UiToggle size="sm" :on="form.autoStart" label="Start when Kova opens" @click="form.autoStart = !form.autoStart" />
            Start when Kova opens
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
.apf-row { display: flex; gap: var(--space-3); align-items: flex-start; }
.apf-row > :first-child { flex: 1; }
.apf-use { margin-top: calc(-1 * var(--space-2)); }
.apf-toggles { display: flex; flex-wrap: wrap; gap: var(--space-4); }
.apf-toggle { display: inline-flex; align-items: center; gap: var(--space-2); font-size: var(--text-sm); color: var(--text); }
.apf-toggle--off { color: var(--faint); }
</style>
