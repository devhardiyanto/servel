<script setup lang="ts">
// Proses tambahan project (queue, scheduler, Vite): status per proses,
// Start/Stop/Restart masing-masing, dan tambah/hapus. Disimpan di
// `project.processes`; backend menjalankannya bersama dev server.
import { computed, ref, watch } from 'vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiIcon from '@/components/ui/UiIcon.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import StatusDot from '@/components/ui/StatusDot.vue'
import { useConfig } from '@/composables/useConfig'
import { useToast } from '@/composables/useToast'
import { useProjectRuntime, isActive } from '@/composables/useProjectRuntime'
import type { Project, ProjectProcess } from '@/types/mise'

const props = defineProps<{ project: Project }>()

const { config, updateProject } = useConfig()
const runtime = useProjectRuntime()
const toast = useToast()

const list = computed<ProjectProcess[]>(() => props.project.processes ?? [])
const busy = ref<string | null>(null)

const NAME_RE = /^[A-Za-z0-9_.:-]{1,24}$/

const suggestions = ref<ProjectProcess[]>([])
async function loadSuggestions(): Promise<void> {
  suggestions.value = []
  if (!props.project.path) return
  try {
    suggestions.value = await runtime.processSuggestions(props.project.path, props.project.command ?? '')
  } catch {
    suggestions.value = []
  }
}
watch(() => [props.project.id, props.project.path, props.project.command], loadSuggestions, { immediate: true })
const unusedSuggestions = computed(() => suggestions.value.filter((s) => !list.value.some((p) => p.name === s.name)))

function latest(): Project {
  return config.value.projects.find((x) => x.id === props.project.id) ?? props.project
}

function stateLabel(name: string): { state: 'running' | 'starting' | 'stopped' | 'exited'; text: string } {
  const w = runtime.workerOf(props.project.id, name)
  if (!w) return { state: 'stopped', text: 'Stopped' }
  if (w.state === 'running') return { state: 'running', text: 'Running' }
  if (w.state === 'starting') return { state: 'starting', text: 'Starting…' }
  if (w.state === 'exited') {
    const code = w.exitCode != null ? ` (exit ${w.exitCode})` : ''
    return { state: 'exited', text: w.gaveUp ? `Keeps crashing${code} · not restarted` : `Exited${code}` }
  }
  return { state: 'stopped', text: 'Stopped' }
}
function active(name: string): boolean {
  const w = runtime.workerOf(props.project.id, name)
  return !!w && isActive(w.state)
}
function restarts(name: string): number {
  return runtime.workerOf(props.project.id, name)?.restarts ?? 0
}
function lastLine(name: string): string | null {
  const w = runtime.workerOf(props.project.id, name)
  return w && w.state === 'exited' ? w.lastLine : null
}

async function run(name: string, what: 'start' | 'stop' | 'restart'): Promise<void> {
  busy.value = name
  try {
    if (what === 'stop') await runtime.stopWorker(props.project.id, name)
    else if (what === 'restart') await runtime.restartWorker(latest(), name)
    else await runtime.startWorker(latest(), name)
  } catch (e) {
    toast.show(`${name}: ${e}`, { tone: 'error' })
  } finally {
    busy.value = null
  }
}

async function save(next: ProjectProcess[]): Promise<void> {
  await updateProject(props.project.id, { processes: next })
}

async function remove(name: string): Promise<void> {
  if (active(name)) await runtime.stopWorker(props.project.id, name)
  await save(list.value.filter((p) => p.name !== name))
  toast.show(`Removed ${name} from ${props.project.name}`)
}

async function toggleAutoRestart(name: string): Promise<void> {
  await save(list.value.map((p) => (p.name === name ? { ...p, autoRestart: !p.autoRestart } : p)))
}

async function addSuggestion(s: ProjectProcess): Promise<void> {
  await save([...list.value, s])
}

// Form tambah manual.
const adding = ref(false)
const newName = ref('')
const newCommand = ref('')
const nameError = computed(() => {
  const n = newName.value.trim()
  if (!n) return ''
  if (!NAME_RE.test(n)) return 'Use 1–24 letters, digits, - _ . or :'
  if (list.value.some((p) => p.name === n)) return `${n} already exists`
  return ''
})
const canAdd = computed(() => !!newName.value.trim() && !!newCommand.value.trim() && !nameError.value)
async function addManual(): Promise<void> {
  if (!canAdd.value) return
  await save([...list.value, { name: newName.value.trim(), command: newCommand.value.trim(), autoRestart: true }])
  newName.value = ''
  newCommand.value = ''
  adding.value = false
}
</script>

<template>
  <section class="pp" aria-labelledby="pp-title">
    <h3 id="pp-title" class="pp-label">Processes</h3>

    <p v-if="!list.length" class="pp-dim">
      Queue workers, schedulers and Vite can run next to the dev server. They start and stop with the project and restart on their own when they crash.
    </p>

    <ul v-else class="pp-list">
      <li v-for="proc in list" :key="proc.name" class="pp-row">
        <div class="pp-top">
          <span class="pp-name">{{ proc.name }}</span>
          <StatusDot :state="stateLabel(proc.name).state" :label="stateLabel(proc.name).text" />
          <span class="pp-sp"></span>
          <template v-if="active(proc.name)">
            <UiButton :disabled="busy === proc.name" :aria-label="`Restart ${proc.name}`" @click="run(proc.name, 'restart')"><UiIcon name="restart" />Restart</UiButton>
            <UiButton :disabled="busy === proc.name" :aria-label="`Stop ${proc.name}`" @click="run(proc.name, 'stop')"><UiIcon name="stop" />Stop</UiButton>
          </template>
          <UiButton v-else :loading="busy === proc.name" :disabled="!project.path" :aria-label="`Start ${proc.name}`" @click="run(proc.name, 'start')"><UiIcon name="play" />Start</UiButton>
          <UiButton variant="quiet" :aria-label="`Remove ${proc.name}`" @click="remove(proc.name)"><UiIcon name="trash" /></UiButton>
        </div>
        <code class="pp-cmd">{{ proc.command }}</code>
        <code v-if="lastLine(proc.name)" class="pp-last">{{ lastLine(proc.name) }}</code>
        <label class="pp-auto">
          <UiToggle size="sm" :label="`Restart ${proc.name} when it crashes`" :on="proc.autoRestart" @click="toggleAutoRestart(proc.name)" />
          <span>Restart when it crashes<template v-if="restarts(proc.name)"> · restarted {{ restarts(proc.name) }}×</template></span>
        </label>
      </li>
    </ul>

    <p v-if="list.length" class="pp-dim">Kova restarts a crashed process up to 5 times in 2 minutes. Output is in Logs, with the process name as a filter.</p>

    <div v-if="unusedSuggestions.length || !adding" class="pp-add">
      <UiButton v-for="s in unusedSuggestions" :key="s.name" :title="s.command" @click="addSuggestion(s)"><UiIcon name="plus" />Add {{ s.name }}</UiButton>
      <UiButton v-if="!adding" variant="quiet" @click="adding = true"><UiIcon name="plus" />Other process…</UiButton>
    </div>

    <form v-if="adding" class="pp-form" @submit.prevent="addManual">
      <UiInput v-model="newName" label="Name" placeholder="e.g. horizon" :error="nameError" width="160px" />
      <UiInput v-model="newCommand" label="Command" placeholder="e.g. php artisan horizon" mono />
      <div class="pp-form-acts">
        <UiButton @click="adding = false">Cancel</UiButton>
        <UiButton type="submit" variant="accent" :disabled="!canAdd">Add process</UiButton>
      </div>
    </form>
  </section>
</template>

<style scoped>
.pp { display: flex; flex-direction: column; gap: var(--space-2); }
.pp-label { margin: 0; font-size: var(--text-xs); font-weight: 500; color: var(--muted); }
.pp-dim { margin: 0; color: var(--faint); font-size: var(--text-xs); }
.pp-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--radius-md); }
.pp-row { display: flex; flex-direction: column; gap: 4px; padding: var(--space-2) var(--space-3); }
.pp-row + .pp-row { border-top: 1px solid var(--border); }
.pp-top { display: flex; align-items: center; gap: var(--space-2); }
.pp-sp { flex: 1; }
.pp-name { font-weight: 600; font-size: var(--text-sm); }
.pp-cmd { font: 400 var(--text-xs) var(--font-mono); color: var(--muted); overflow-wrap: anywhere; }
.pp-last { font: 400 var(--text-xs) var(--font-mono); color: var(--red); overflow-wrap: anywhere; }
.pp-auto { display: inline-flex; align-items: center; gap: var(--space-2); font-size: var(--text-xs); color: var(--muted); align-self: flex-start; }
.pp-add { display: flex; flex-wrap: wrap; gap: var(--space-2); }
.pp-form { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); }
.pp-form-acts { display: flex; justify-content: flex-end; gap: var(--space-2); }
</style>
