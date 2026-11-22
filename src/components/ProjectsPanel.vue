<script setup lang="ts">
// Daftar project: kartu ringkas dalam kelompok, dipilih untuk dilihat di panel
// detail. Aksi project ada di panel detail; di sini hanya urutan dan kelompok.
// Urutan diubah dengan menyeret pegangan ⋮⋮ (pointer events, karena drag HTML5
// tidak konsisten di WebView2 dan WebKitGTK) atau Alt+↑/↓ di pegangan itu.
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import UiButton from '@/components/ui/UiButton.vue'
import UiMenu, { type MenuItem } from '@/components/ui/UiMenu.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusDot from '@/components/ui/StatusDot.vue'
import FrameworkChip from '@/components/FrameworkChip.vue'
import { useConfig } from '@/composables/useConfig'
import { isActive, useProjectRuntime } from '@/composables/useProjectRuntime'
import { canRun, useProjectActions } from '@/composables/useProjectActions'
import { useToast } from '@/composables/useToast'
import { groupIdOf } from '@/utils/projectGroups'
import type { Project, ProjectGroup } from '@/types/mise'

const props = defineProps<{ selectedId?: string | null; query?: string; detailsOpen?: boolean }>()
const emit = defineEmits<{ select: [id: string] }>()

const { config, createGroup, renameGroup, deleteGroup, setGroupCollapsed } = useConfig()
const { stateOf, infoOf } = useProjectRuntime()
const actions = useProjectActions()
const toast = useToast()

/** Environment proses yang jalan, bila bukan Default. */
function envOf(id: string): string {
  const i = infoOf(id)
  return i && (i.state === 'running' || i.state === 'starting' || i.state === 'unresponsive') ? (i.envName ?? '') : ''
}

const all = computed<Project[]>(() => config.value.projects)
const groups = computed<ProjectGroup[]>(() => config.value.groups ?? [])
const hasGroups = computed(() => groups.value.length > 0)

const words = computed(() => (props.query ?? '').toLowerCase().split(/\s+/).filter(Boolean))
const searching = computed(() => words.value.length > 0)
function matches(p: Project): boolean {
  const g = groups.value.find((x) => x.id === groupIdOf(p, groups.value))?.name ?? ''
  const hay = `${p.name} ${p.path} ${p.kind ?? ''} ${p.port ?? ''} ${g}`.toLowerCase()
  return words.value.every((w) => hay.includes(w))
}
const matchCount = computed(() => all.value.filter(matches).length)

interface Section {
  /** '' = Ungrouped. */
  key: string
  group: ProjectGroup | null
  projects: Project[]
  total: Project[]
}

// Kelompok kosong tetap tampil (tempat menaruh project). Ungrouped tampil bila
// berisi, bila tidak ada kelompok, atau selama menyeret (supaya bisa dituju).
const sections = computed<Section[]>(() => {
  const gs = groups.value
  const out: Section[] = gs.map((g) => {
    const total = all.value.filter((p) => p.groupId === g.id)
    return { key: g.id, group: g, total, projects: searching.value ? total.filter(matches) : total }
  })
  const total = all.value.filter((p) => groupIdOf(p, gs) === null)
  if (total.length || gs.length === 0 || drag.value) out.push({ key: '', group: null, total, projects: searching.value ? total.filter(matches) : total })
  return searching.value ? out.filter((s) => s.projects.length > 0) : out
})

function expanded(s: Section): boolean {
  return searching.value || !s.group?.collapsed
}
function runningIn(s: Section): number {
  return s.total.filter((p) => isActive(stateOf(p.id))).length
}
function metaOf(s: Section): string {
  if (!s.total.length) return 'empty'
  const r = runningIn(s)
  return `${s.total.length}${r ? ` · ${r} running` : ''}`
}
function canStartAll(s: Section): boolean {
  return s.total.some((p) => canRun(p) && !isActive(stateOf(p.id)))
}

function groupMenu(s: Section): MenuItem[] {
  return [
    { id: 'rename', label: 'Rename' },
    { id: 'stop', label: 'Stop all', disabled: runningIn(s) === 0 },
    { id: 'delete', label: 'Delete group', separator: true, danger: true, hint: s.total.length ? 'projects move to Ungrouped' : undefined },
  ]
}
async function onGroupMenu(s: Section, id: string): Promise<void> {
  const g = s.group
  if (!g) return
  if (id === 'rename') startRename(g)
  else if (id === 'stop') actions.stopGroup(g.id)
  else if (id === 'delete') {
    await deleteGroup(g.id)
    toast.show(s.total.length ? `Deleted ${g.name}. Its projects are in Ungrouped.` : `Deleted ${g.name}.`)
  }
}

// --- Nama kelompok (baru atau diubah) -----------------------------------------
const nameInput = ref<HTMLInputElement[] | HTMLInputElement | null>(null)
const nameDraft = ref('')
function focusName(): void {
  void nextTick(() => {
    const el = Array.isArray(nameInput.value) ? nameInput.value[0] : nameInput.value
    el?.focus()
    el?.select()
  })
}
/** Dipanggil layar Projects (tombol New group, palet perintah). */
function startNewGroup(projectId: string | null = null): void {
  actions.renamingGroupId.value = null
  actions.draftGroup.value = { projectId }
}
function startRename(g: ProjectGroup): void {
  actions.draftGroup.value = null
  nameDraft.value = g.name
  actions.renamingGroupId.value = g.id
  focusName()
}
// Draft bisa juga dibuka dari menu ⋯ panel detail atau palet perintah.
const draftOpen = computed(() => actions.draftGroup.value !== null)
watch(
  () => actions.draftGroup.value,
  (d) => {
    if (!d) return
    nameDraft.value = ''
    focusName()
  },
  { immediate: true },
)
let saving = false
async function commitName(): Promise<void> {
  if (saving) return
  const name = nameDraft.value.trim()
  const draft = actions.draftGroup.value
  const renaming = actions.renamingGroupId.value
  if (draft) {
    actions.draftGroup.value = null
    if (!name) return
    saving = true
    try {
      const g = await createGroup(name, draft.projectId ?? undefined)
      if (draft.projectId) {
        const p = all.value.find((x) => x.id === draft.projectId)
        if (p) toast.show(`Moved ${p.name} to ${g.name}`)
      }
    } finally {
      saving = false
    }
  } else if (renaming) {
    actions.renamingGroupId.value = null
    if (name) await renameGroup(renaming, name)
  }
}
function cancelName(): void {
  actions.draftGroup.value = null
  actions.renamingGroupId.value = null
}

// --- Seret untuk mengurutkan -------------------------------------------------
interface Drag {
  id: string
  pointerId: number
  startY: number
  dy: number
  active: boolean
  /** Tujuan: kelompok ('' = Ungrouped) dan project yang didahului (null = akhir). */
  target: { key: string; before: string | null } | null
}
const drag = ref<Drag | null>(null)
const wrap = ref<HTMLElement | null>(null)

function onGripDown(e: PointerEvent, p: Project): void {
  if (e.button !== 0) return
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  drag.value = { id: p.id, pointerId: e.pointerId, startY: e.clientY, dy: 0, active: false, target: null }
  document.addEventListener('keydown', onDragKey, true)
}

function computeTarget(y: number): Drag['target'] {
  const secs = Array.from(wrap.value?.querySelectorAll<HTMLElement>('[data-sec]') ?? [])
  if (!secs.length) return null
  let sec = secs.find((el) => {
    const r = el.getBoundingClientRect()
    return y >= r.top && y <= r.bottom
  })
  if (!sec) sec = y < secs[0].getBoundingClientRect().top ? secs[0] : secs[secs.length - 1]
  const key = sec.dataset.sec ?? ''
  const cards = Array.from(sec.querySelectorAll<HTMLElement>('[data-id]')).filter((el) => el.dataset.id !== drag.value?.id)
  const next = cards.find((el) => {
    const r = el.getBoundingClientRect()
    return y < r.top + r.height / 2
  })
  return { key, before: next?.dataset.id ?? null }
}

function onGripMove(e: PointerEvent): void {
  const d = drag.value
  if (!d || e.pointerId !== d.pointerId) return
  d.dy = e.clientY - d.startY
  if (!d.active && Math.abs(d.dy) < 4) return
  d.active = true
  d.target = computeTarget(e.clientY)
  // Gulir daftar bila pointer di dekat tepi atas/bawahnya.
  const scroller = wrap.value?.closest<HTMLElement>('.pv-list')
  if (scroller) {
    const r = scroller.getBoundingClientRect()
    if (e.clientY < r.top + 40) scroller.scrollBy(0, -12)
    else if (e.clientY > r.bottom - 40) scroller.scrollBy(0, 12)
  }
}

function endDrag(): void {
  drag.value = null
  document.removeEventListener('keydown', onDragKey, true)
}

async function onGripUp(e: PointerEvent): Promise<void> {
  const d = drag.value
  if (!d || e.pointerId !== d.pointerId) return
  const target = d.active ? d.target : null
  endDrag()
  const p = all.value.find((x) => x.id === d.id)
  if (!p || !target) return
  await actions.moveTo(p, target.key || null, target.before)
}

function onDragKey(e: KeyboardEvent): void {
  if (e.key === 'Escape' && drag.value) {
    e.preventDefault()
    e.stopPropagation()
    endDrag()
  }
}
onBeforeUnmount(endDrag)

function dropBefore(key: string, id: string | null): boolean {
  const t = drag.value?.active ? drag.value.target : null
  return !!t && t.key === key && t.before === id
}

async function onGripKey(e: KeyboardEvent, p: Project): Promise<void> {
  if (!e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
  e.preventDefault()
  await actions.shift(p, e.key === 'ArrowUp' ? -1 : 1)
  await nextTick()
  wrap.value?.querySelector<HTMLElement>(`[data-grip="${p.id}"]`)?.focus()
}

async function howDetection(): Promise<void> {
  const url = 'https://github.com/devhardiyanto/servel#projects'
  try {
    await openUrl(url)
  } catch {
    window.open(url, '_blank', 'noopener')
  }
}

defineExpose({ startNewGroup })
</script>

<template>
  <EmptyState v-if="all.length === 0 && !hasGroups && !draftOpen" title="No projects yet" icon="folder">
    Add a project folder. Kova detects Laravel, Django, Nuxt, Vite, React, Next, Hono, Express, Go and more, then runs it on a fixed localhost port.
    <template #actions>
      <UiButton variant="primary" size="md" @click="actions.adding.value = true">Add project</UiButton>
      <UiButton variant="quiet" size="md" @click="howDetection">How detection works</UiButton>
    </template>
  </EmptyState>

  <EmptyState v-else-if="searching && matchCount === 0" title="No project matches" icon="search">
    Nothing matches "{{ query }}".
  </EmptyState>

  <div v-else ref="wrap" class="pl-wrap" :class="{ 'pl-wrap--drag': drag?.active }">
    <section
      v-for="s in sections"
      :key="s.key || '~ungrouped'"
      class="pl-sec"
      :data-sec="s.key"
      :aria-label="hasGroups ? (s.group?.name ?? 'Ungrouped') : undefined"
    >
      <header v-if="hasGroups" class="pl-gh">
        <button
          v-if="s.group"
          type="button"
          class="pl-caret"
          :aria-expanded="expanded(s)"
          :aria-controls="`pl-g-${s.key}`"
          :aria-label="`${expanded(s) ? 'Collapse' : 'Expand'} ${s.group.name}`"
          :disabled="searching"
          @click="setGroupCollapsed(s.group.id, !s.group.collapsed)"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true" :class="{ 'pl-caret--open': expanded(s) }"><path d="M6 4l4 4-4 4" /></svg>
        </button>
        <span v-else class="pl-caret pl-caret--none" aria-hidden="true"></span>
        <input
          v-if="s.group && actions.renamingGroupId.value === s.group.id"
          ref="nameInput"
          v-model="nameDraft"
          class="pl-gname-in"
          :aria-label="`Rename ${s.group.name}`"
          maxlength="60"
          @keydown.enter.prevent="commitName"
          @keydown.esc.prevent.stop="cancelName"
          @blur="commitName"
        />
        <h2 v-else class="pl-gname">{{ s.group?.name ?? 'Ungrouped' }}</h2>
        <span class="pl-gmeta">{{ metaOf(s) }}</span>
        <span class="pl-sp"></span>
        <UiButton v-if="s.group && canStartAll(s)" variant="quiet" :aria-label="`Start all in ${s.group.name}`" @click="actions.startGroup(s.group.id)">Start all</UiButton>
        <UiMenu v-if="s.group" :items="groupMenu(s)" :label="`Actions for group ${s.group.name}`" @select="onGroupMenu(s, $event)" />
      </header>

      <ul v-if="expanded(s)" :id="`pl-g-${s.key}`" class="pl" :aria-label="hasGroups ? `Projects in ${s.group?.name ?? 'Ungrouped'}` : 'Projects'">
        <template v-for="p in s.projects" :key="p.id">
          <li v-if="dropBefore(s.key, p.id)" class="pl-drop" aria-hidden="true"></li>
          <li
            class="pl-li"
            :data-id="p.id"
            :class="{ 'pl-li--drag': drag?.active && drag.id === p.id }"
            :style="drag?.active && drag.id === p.id ? { transform: `translateY(${drag.dy}px)` } : undefined"
          >
            <button
              type="button"
              class="pl-grip"
              :data-grip="p.id"
              :aria-label="`Reorder ${p.name}. Alt+Up or Alt+Down to move`"
              @pointerdown="onGripDown($event, p)"
              @pointermove="onGripMove"
              @pointerup="onGripUp"
              @pointercancel="endDrag"
              @keydown="onGripKey($event, p)"
            >
              <svg viewBox="0 0 16 28" aria-hidden="true"><circle cx="5.5" cy="9" r="1.3" /><circle cx="10.5" cy="9" r="1.3" /><circle cx="5.5" cy="14" r="1.3" /><circle cx="10.5" cy="14" r="1.3" /><circle cx="5.5" cy="19" r="1.3" /><circle cx="10.5" cy="19" r="1.3" /></svg>
            </button>
            <button
              type="button"
              class="pl-card"
              :class="{ 'pl-card--sel': selectedId === p.id }"
              :aria-pressed="selectedId === p.id && detailsOpen ? 'true' : 'false'"
              @click="emit('select', p.id)"
            >
              <StatusDot class="pl-dot" :state="stateOf(p.id)" />
              <span class="pl-main">
                <span class="pl-name">{{ p.name }}</span>
                <span class="pl-path">{{ p.path || 'Folder not chosen yet' }}</span>
              </span>
              <FrameworkChip v-if="p.kind" :kind="p.kind" />
              <span v-else></span>
              <span class="pl-port">{{ (p.port ?? 0) > 0 ? `:${p.port}` : '' }}<template v-if="envOf(p.id)"> · {{ envOf(p.id) }}</template></span>
              <span class="pl-state" :class="`pl-state--${stateOf(p.id)}`">{{ actions.stateLabel(p) }}</span>
            </button>
          </li>
        </template>
        <li v-if="dropBefore(s.key, null)" class="pl-drop" aria-hidden="true"></li>
        <li v-if="!s.total.length && s.group" class="pl-hint">Drag projects here, or use ⋯ → Move to group.</li>
      </ul>
    </section>

    <section v-if="draftOpen" class="pl-sec" aria-label="New group">
      <header class="pl-gh">
        <span class="pl-caret pl-caret--none" aria-hidden="true"></span>
        <input
          ref="nameInput"
          v-model="nameDraft"
          class="pl-gname-in"
          aria-label="New group name"
          placeholder="Group name"
          maxlength="60"
          @keydown.enter.prevent="commitName"
          @keydown.esc.prevent.stop="cancelName"
          @blur="commitName"
        />
        <span class="pl-gmeta">Enter to save · Esc to cancel</span>
      </header>
      <ul class="pl"><li class="pl-hint">Drag projects here, or use ⋯ → Move to group.</li></ul>
    </section>
  </div>
</template>

<style scoped>
.pl-wrap { display: flex; flex-direction: column; gap: var(--space-4); }
.pl-wrap--drag, .pl-wrap--drag * { cursor: grabbing !important; user-select: none; }
.pl-sec { display: flex; flex-direction: column; gap: var(--space-2); }
.pl-gh { display: flex; align-items: center; gap: var(--space-2); min-height: 28px; }
.pl-caret {
  flex: none;
  width: 22px;
  height: 22px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border: 0;
  border-radius: var(--radius-xs);
  background: none;
  color: var(--muted);
  cursor: pointer;
}
.pl-caret:hover:not(:disabled) { background: var(--surface2); color: var(--text); }
.pl-caret:disabled { cursor: default; }
.pl-caret:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.pl-caret svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; transition: transform var(--dur-fast) var(--ease); }
.pl-caret--open { transform: rotate(90deg); }
.pl-caret--none { cursor: default; }
.pl-gname { margin: 0; font-size: var(--text-xs); font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); }
.pl-gname-in {
  min-width: 160px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--accent);
  border-radius: var(--radius-xs);
  background: var(--surface2);
  color: var(--text);
  font: 600 var(--text-sm) var(--font-sans);
}
.pl-gname-in:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.pl-gmeta { font: 400 var(--text-xs) var(--font-mono); color: var(--faint); }
.pl-sp { flex: 1; }

.pl-dot { flex: none; }
.pl-state { font-size: var(--text-xs); color: var(--muted); white-space: nowrap; }
.pl-state--running { color: var(--green); }
.pl-state--starting, .pl-state--unresponsive { color: var(--amber); }
.pl-state--exited { color: var(--red); }

.pl { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-2); }
.pl-li { position: relative; display: flex; align-items: center; gap: 2px; margin-left: -18px; }
.pl-grip {
  flex: none;
  width: 16px;
  height: 28px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-xs);
  background: none;
  color: var(--muted);
  cursor: grab;
  opacity: 0;
  touch-action: none;
  transition: opacity var(--dur-fast) var(--ease);
}
.pl-grip svg { width: 16px; height: 28px; fill: currentColor; }
.pl-li:hover .pl-grip, .pl-grip:focus-visible, .pl-li--drag .pl-grip { opacity: 1; }
.pl-grip:hover { color: var(--text); background: var(--surface2); }
.pl-grip:focus-visible { outline: 2px solid var(--accent); outline-offset: 0; }
.pl-li--drag { z-index: 2; pointer-events: none; }
.pl-li--drag .pl-card {
  background: var(--surface2);
  border-color: var(--accent);
  box-shadow: 0 12px 28px -8px rgba(0, 0, 0, 0.6);
}
.pl-drop { position: relative; z-index: 3; height: 2px; margin: -5px 0 -5px 18px; border-radius: 1px; background: var(--accent); }
.pl-hint {
  padding: var(--space-3);
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  color: var(--muted);
  text-align: center;
}
.pl-card {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto minmax(64px, max-content) 96px;
  align-items: center;
  gap: var(--space-3);
  min-height: 56px;
  padding: 9px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
}
.pl-card:hover { border-color: var(--border-strong); }
.pl-card:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.pl-card--sel { border-color: color-mix(in srgb, var(--accent) 55%, transparent); background: var(--surface2); }
.pl-main { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.pl-name { font-size: var(--text-md); font-weight: 600; }
.pl-path { font: 400 var(--text-xs) var(--font-mono); color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.pl-port { font: 400 var(--text-xs) var(--font-mono); color: var(--muted); text-align: right; }
.pl-state { justify-self: end; }
@media (prefers-reduced-motion: reduce) { .pl-caret svg, .pl-grip { transition: none; } }
@media (max-width: 760px) {
  .pl-card { grid-template-columns: auto minmax(0, 1fr) auto; }
  .pl-card > :nth-child(3), .pl-port { display: none; }
}
</style>
