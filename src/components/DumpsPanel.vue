<script setup lang="ts">
// Tab Dumps (Experimental): dump() dan dd() dari dev server dan proses
// pendamping project ini, terbaru di atas. Isi dump adalah teks CliDumper.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import UiButton from '@/components/ui/UiButton.vue'
import type { Dump, DumpsStatus } from '@/composables/useDumps'

const props = defineProps<{ projectId: string; projectPath: string; running: boolean }>()
const emit = defineEmits<{ status: [s: DumpsStatus] }>()

const items = ref<Dump[]>([])
const status = ref<DumpsStatus | null>(null)
const paused = ref(false)
const error = ref('')
const newest = computed(() => [...items.value].reverse())

async function refresh(): Promise<void> {
  try {
    status.value = await invoke<DumpsStatus>('dumps_status', { projectId: props.projectId, projectPath: props.projectPath })
    emit('status', status.value)
    if (!paused.value) items.value = await invoke<Dump[]>('dumps_list', { projectId: props.projectId })
  } catch (e) {
    error.value = String(e)
  }
}

async function clear(): Promise<void> {
  await invoke('dumps_clear', { projectId: props.projectId })
  items.value = []
}

async function openSource(d: Dump): Promise<void> {
  if (!d.file) return
  error.value = ''
  try {
    await invoke('dumps_open_source', { projectPath: props.projectPath, file: d.file, line: d.line })
  } catch (e) {
    error.value = String(e)
  }
}

function where(d: Dump): string {
  if (d.request?.uri) return `${d.request.method ?? ''} ${d.request.uri}`.trim()
  if (d.command) return d.command
  return ''
}
function rel(file: string): string {
  const root = props.projectPath.replace(/[\\/]+$/, '')
  return file.toLowerCase().startsWith(root.toLowerCase()) ? file.slice(root.length + 1) : file
}
function clock(t: number): string {
  return new Date(t * 1000).toLocaleTimeString([], { hour12: false })
}

const stateText = computed(() => {
  const s = status.value
  if (!s) return ''
  if (s.state === 'listening') return `Listening · PHP ${s.php ?? '?'}${s.varDumper ? ` · var-dumper ${s.varDumper}` : ''}`
  if (s.state === 'starting') return 'Starting the receiver…'
  if (s.state === 'failed') return `Receiver stopped: ${s.error ?? 'unknown error'}`
  return props.running ? 'Restart the project to start catching dumps.' : 'Start the project to catch dumps.'
})

let offNew: UnlistenFn | null = null
let offStatus: UnlistenFn | null = null
onMounted(async () => {
  await refresh()
  offNew = await listen<{ projectId: string; dump: Dump }>('dumps://new', ({ payload }) => {
    if (payload.projectId !== props.projectId || paused.value) return
    items.value.push(payload.dump)
    if (items.value.length > 200) items.value.shift()
  })
  offStatus = await listen<{ projectId: string }>('dumps://status', ({ payload }) => {
    if (payload.projectId === props.projectId) void refresh()
  })
})
watch(() => props.projectId, () => { items.value = []; void refresh() })
watch(paused, (p) => { if (!p) void refresh() })
onBeforeUnmount(() => { offNew?.(); offStatus?.() })
</script>

<template>
  <div class="dp">
    <div class="dp-bar">
      <span class="dp-state" :class="status?.state" role="status">{{ stateText }}</span>
      <UiButton :aria-pressed="paused" @click="paused = !paused">{{ paused ? 'Resume' : 'Pause' }}</UiButton>
      <UiButton :disabled="!items.length" @click="clear">Clear</UiButton>
    </div>
    <p v-if="error" class="dp-err" role="alert">{{ error }}</p>
    <p v-if="!newest.length" class="dp-empty">
      No dumps yet. Call <code>dump($value)</code> or <code>dd($value)</code> in this project; they show up here instead of in the page.
    </p>
    <ol v-else class="dp-list">
      <li v-for="(d, i) in newest" :key="`${d.time}-${i}`" class="dp-item">
        <div class="dp-meta">
          <span class="dp-time">{{ clock(d.time) }}</span>
          <span v-if="where(d)" class="dp-where" :title="where(d)">{{ where(d) }}</span>
          <button v-if="d.file" type="button" class="dp-src" :title="`Open ${d.file} in VS Code`" @click="openSource(d)">
            {{ rel(d.file) }}<template v-if="d.line">:{{ d.line }}</template>
          </button>
          <span v-else-if="d.name" class="dp-src dp-src--plain">{{ d.name }}<template v-if="d.line">:{{ d.line }}</template></span>
        </div>
        <pre class="dp-text">{{ d.text }}</pre>
        <p v-if="d.cut" class="dp-cut">Cut at 256 KB.</p>
      </li>
    </ol>
  </div>
</template>

<style scoped>
.dp { display: flex; flex-direction: column; gap: var(--space-3); min-width: 0; }
.dp-bar { display: flex; gap: var(--space-2); align-items: center; }
.dp-state { margin-right: auto; font-size: var(--text-xs); color: var(--muted); }
.dp-state.failed { color: var(--red); }
.dp-err { margin: 0; color: var(--red); font-size: var(--text-xs); }
.dp-empty { margin: 0; color: var(--muted); font-size: var(--text-sm); }
.dp-empty code { font-family: var(--font-mono); font-size: var(--text-xs); }
.dp-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-2); }
.dp-item { border: 1px solid var(--border); border-radius: var(--radius-xs); background: var(--bg-sunken); min-width: 0; }
.dp-meta { display: flex; gap: var(--space-2); align-items: baseline; padding: 6px 10px; border-bottom: 1px solid var(--border); font-size: var(--text-xs); min-width: 0; }
.dp-time { color: var(--faint); font-variant-numeric: tabular-nums; }
.dp-where { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
.dp-src { margin-left: auto; flex: none; max-width: 50%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: 0; background: none; padding: 0; color: var(--accent); font: inherit; font-family: var(--font-mono); cursor: pointer; }
.dp-src--plain { color: var(--muted); cursor: default; }
.dp-src:hover { text-decoration: underline; }
.dp-src:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.dp-text { margin: 0; padding: 8px 10px; font-family: var(--font-mono); font-size: var(--text-xs); line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 320px; overflow: auto; color: var(--text); }
.dp-cut { margin: 0; padding: 0 10px 6px; color: var(--faint); font-size: var(--text-xs); }
</style>
