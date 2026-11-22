<script setup lang="ts">
// Palet perintah: Ctrl K / Cmd K. Pindah layar, nyalakan atau matikan service,
// start/stop/restart/buka project dan kelompok project. Daftar dibangun dari
// state yang sama dengan layar lain, jadi labelnya mengikuti keadaan saat ini.
// Project mengikuti urutan layar Projects (kelompok, lalu Ungrouped).
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useConfig } from '@/composables/useConfig'
import { useServices } from '@/composables/useServices'
import { isActive, useProjectRuntime } from '@/composables/useProjectRuntime'
import { canRun, useProjectActions } from '@/composables/useProjectActions'
import { PROJECTS_PANEL, usePanelWidth } from '@/composables/usePanelWidth'
import FrameworkLogo from '@/components/ui/FrameworkLogo.vue'
import { filterCommands, projectCommandMeta, type PaletteCommand } from '@/utils/palette'
import { groupIdOf, orderedProjects } from '@/utils/projectGroups'
import type { ViewName } from '@/types/navigation'

const props = defineProps<{ view?: ViewName | null }>()
const emit = defineEmits<{ go: [view: ViewName] }>()

const open = ref(false)
const query = ref('')
const active = ref(0)
const error = ref('')
const inputEl = ref<HTMLInputElement | null>(null)
const listEl = ref<HTMLElement | null>(null)
let lastFocus: HTMLElement | null = null

const { config } = useConfig()
const { definitions, uiState, selectedCount, runningCount, toggle, start, stop, stopAll } = useServices()
const projectRt = useProjectRuntime()
const actions = useProjectActions()
const panel = usePanelWidth('projects', PROJECTS_PANEL)

const VIEWS: { id: ViewName; label: string }[] = [
  { id: 'projects', label: 'Projects' },
  { id: 'services', label: 'Services' },
  { id: 'runtimes', label: 'Runtimes' },
  { id: 'logs', label: 'Logs' },
  { id: 'settings', label: 'Settings' },
]

const commands = computed<PaletteCommand[]>(() => {
  const list: PaletteCommand[] = []

  const groups = config.value.groups ?? []
  for (const p of orderedProjects(config.value.projects, groups)) {
    if (!p.command || !p.path) continue
    const running = isActive(projectRt.stateOf(p.id))
    const { group, keywords: kw } = projectCommandMeta(p, groups)
    const kind = p.kind
    if (running) {
      if (p.port) {
        list.push({ id: `p-open-${p.id}`, label: `Open ${p.name}`, group: `localhost:${p.port}`, keywords: `${kw} browser`, kind, run: () => openUrl(`http://localhost:${p.port}`) })
      }
      list.push({ id: `p-restart-${p.id}`, label: `Restart ${p.name}`, group, keywords: kw, kind, run: () => projectRt.restart(p) })
      list.push({ id: `p-stop-${p.id}`, label: `Stop ${p.name}`, group, keywords: kw, kind, run: () => projectRt.stop(p.id) })
    } else {
      list.push({ id: `p-start-${p.id}`, label: `Start ${p.name}`, group, keywords: `${kw} run`, kind, run: () => projectRt.start(p) })
    }
  }

  for (const g of groups) {
    const inGroup = config.value.projects.filter((p) => groupIdOf(p, groups) === g.id)
    if (inGroup.some((p) => canRun(p) && !isActive(projectRt.stateOf(p.id)))) {
      list.push({ id: `g-start-${g.id}`, label: `Start group ${g.name}`, group: 'Group', keywords: 'all projects run', run: () => actions.startGroup(g.id) })
    }
    if (inGroup.some((p) => isActive(projectRt.stateOf(p.id)))) {
      list.push({ id: `g-stop-${g.id}`, label: `Stop group ${g.name}`, group: 'Group', keywords: 'all projects', run: () => actions.stopGroup(g.id) })
    }
  }
  list.push({
    id: 'g-new',
    label: 'New group',
    group: 'Projects',
    keywords: 'create group folder organize',
    run: () => {
      actions.renamingGroupId.value = null
      actions.draftGroup.value = { projectId: null }
      emit('go', 'projects')
    },
  })
  if (props.view === 'projects' && config.value.projects.length) {
    const hidden = panel.collapsed.value
    list.push({
      id: 'p-details',
      label: hidden ? 'Show project details' : 'Hide project details',
      group: 'Projects',
      keywords: 'panel detail close open toggle',
      run: () => { panel.collapsed.value = !hidden },
    })
  }

  if (selectedCount.value > 0) {
    list.push({ id: 's-start', label: 'Start selected services', group: 'Services', keywords: 'up all', run: async () => { await start() } })
  }
  if (runningCount.value > 0) {
    list.push({ id: 's-stop-all', label: 'Stop all services', group: 'Services', keywords: 'down', run: () => stopAll() })
  }
  for (const def of definitions.value) {
    const st = uiState.value[def.id]
    if (!st) continue
    const up = st.status === 'running' || st.status === 'starting'
    if (st.status === 'stopping') continue
    list.push({
      id: `s-${def.id}`,
      label: up ? `Stop ${def.name}` : `Start ${def.name}`,
      group: 'Service',
      keywords: `${def.id} ${up ? 'down off' : 'up on'}`,
      // Sama dengan switch di layar Services: start ikut memilih service,
      // stop ikut melepas pilihannya.
      run: () => {
        if (up) return st.selected ? toggle(def.id) : stop([def.id])
        if (st.selected) return start([def.id]).then(() => {})
        return toggle(def.id)
      },
    })
  }

  for (const v of VIEWS) {
    list.push({ id: `go-${v.id}`, label: `Go to ${v.label}`, group: 'Navigate', keywords: 'open show view', run: () => emit('go', v.id) })
  }
  return list
})

const results = computed(() => filterCommands(commands.value, query.value))

watch(query, () => { active.value = 0 })
watch(results, (r) => { if (active.value >= r.length) active.value = Math.max(0, r.length - 1) })

function show(): void {
  lastFocus = document.activeElement as HTMLElement | null
  query.value = ''
  active.value = 0
  error.value = ''
  open.value = true
  void nextTick(() => inputEl.value?.focus())
}

function hide(): void {
  open.value = false
  lastFocus?.focus?.()
}

async function run(cmd: PaletteCommand | undefined): Promise<void> {
  if (!cmd) return
  try {
    await cmd.run()
    hide()
  } catch (err) {
    error.value = String(err)
  }
}

function move(delta: number): void {
  const n = results.value.length
  if (n === 0) return
  active.value = (active.value + delta + n) % n
  void nextTick(() => listEl.value?.querySelector('.cp-item--on')?.scrollIntoView({ block: 'nearest' }))
}

function onGlobalKey(e: KeyboardEvent): void {
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === 'k') {
    e.preventDefault()
    if (open.value) hide()
    else show()
  }
}

function onKey(e: KeyboardEvent): void {
  if (e.key === 'ArrowDown') { e.preventDefault(); move(1) }
  else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1) }
  else if (e.key === 'Enter') { e.preventDefault(); void run(results.value[active.value]) }
  else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); hide() }
  else if (e.key === 'Tab') { e.preventDefault() }
}

onMounted(() => document.addEventListener('keydown', onGlobalKey))
onUnmounted(() => document.removeEventListener('keydown', onGlobalKey))

defineExpose({ show })
</script>

<template>
  <div v-if="open" class="cp-overlay" @mousedown.self="hide">
    <div class="cp" role="dialog" aria-modal="true" aria-label="Command palette" @keydown="onKey">
      <input
        ref="inputEl"
        v-model="query"
        class="cp-input"
        type="text"
        placeholder="Type a command, project or service"
        spellcheck="false"
        autocomplete="off"
        role="combobox"
        aria-expanded="true"
        aria-controls="cp-list"
        :aria-activedescendant="results.length ? `cp-opt-${active}` : undefined"
      />
      <ul id="cp-list" ref="listEl" class="cp-list" role="listbox" aria-label="Commands">
        <li
          v-for="(cmd, i) in results"
          :id="`cp-opt-${i}`"
          :key="cmd.id"
          class="cp-item"
          :class="{ 'cp-item--on': i === active }"
          role="option"
          :aria-selected="i === active"
          @mousemove="active = i"
          @click="run(cmd)"
        >
          <span class="cp-label"><FrameworkLogo v-if="cmd.kind" class="cp-logo" :kind="cmd.kind" :size="13" />{{ cmd.label }}</span>
          <span class="cp-group">{{ cmd.group }}</span>
        </li>
        <li v-if="results.length === 0" class="cp-empty">No command matches "{{ query }}".</li>
      </ul>
      <p v-if="error" class="cp-error" role="alert">{{ error }}</p>
      <div class="cp-foot">
        <span><kbd>↑</kbd><kbd>↓</kbd> move</span>
        <span><kbd>Enter</kbd> run</span>
        <span><kbd>Esc</kbd> close</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cp-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 12vh;
  background: rgba(0, 0, 0, 0.45);
  z-index: 60;
}
.cp {
  width: min(560px, calc(100vw - 32px));
  display: flex;
  flex-direction: column;
  background: var(--surface);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  overflow: hidden;
}
.cp-input {
  padding: var(--space-4);
  background: transparent;
  border: 0;
  border-bottom: 1px solid var(--border);
  color: var(--text);
  font-size: var(--text-md);
  outline: none;
}
.cp-input::placeholder { color: var(--faint); }
.cp-list {
  list-style: none;
  margin: 0;
  padding: var(--space-2);
  max-height: 46vh;
  overflow-y: auto;
}
.cp-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--muted);
  cursor: pointer;
}
.cp-item--on { background: var(--surface2); color: var(--text); }
.cp-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cp-logo { margin-right: 8px; vertical-align: -2px; }
.cp-group { flex-shrink: 0; font-family: var(--font-mono); font-size: var(--text-xs); color: var(--faint); }
.cp-empty { padding: var(--space-4) var(--space-3); font-size: var(--text-sm); color: var(--faint); }
.cp-error { margin: 0; padding: var(--space-2) var(--space-4); font-size: var(--text-xs); color: var(--red); }
.cp-foot {
  display: flex;
  gap: var(--space-4);
  padding: var(--space-2) var(--space-4);
  border-top: 1px solid var(--border);
  font-size: var(--text-xs);
  color: var(--faint);
}
.cp-foot kbd {
  display: inline-block;
  margin-right: 4px;
  padding: 0 5px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-xs);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--muted);
}
</style>
