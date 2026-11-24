<script setup lang="ts">
import UiTabs from '@/components/ui/UiTabs.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiIcon from '@/components/ui/UiIcon.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import { useLogFocus } from '@/composables/useLogFocus'
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useLogs } from '@/composables/useLogs'
import { useLogCopy } from '@/composables/useLogCopy'
import { badgeStyle } from '@/utils/logColors'
import { useConfig } from '@/composables/useConfig'
import UiBanner from '@/components/ui/UiBanner.vue'
import { useToast } from '@/composables/useToast'
import { useProjectActions } from '@/composables/useProjectActions'
import { useProjectRuntime } from '@/composables/useProjectRuntime'

const { lines, clear } = useLogs()

const TABS = ['All', 'Projects', 'MySQL', 'PostgreSQL', 'Redis', 'PHP', 'Node', 'mise'] as const
type Tab = (typeof TABS)[number]

const activeTab = ref<Tab>('All')
const autoScroll = ref(true)
const logAreaRef = ref<HTMLElement | null>(null)

const TAB_SRC: Record<string, string[]> = {
  MySQL: ['MYSQL'],
  PostgreSQL: ['POSTGRES', 'PGSQL'],
  Redis: ['REDIS'],
  PHP: ['PHP'],
  Node: ['NODE'],
  mise: ['MISE'],
}

const TAB_KEYWORDS: Record<string, string[]> = {
  MySQL: ['mysql', 'servel_mysql'],
  PostgreSQL: ['postgres', 'servel_postgres'],
  Redis: ['redis', 'servel_redis'],
  PHP: ['phpvm', 'php '],
  Node: ['fnm', 'node '],
  mise: ['mise '],
}

// Filter di tab Projects: satu project saja, atau semua ('').
const projectFilter = ref('')
const toast = useToast()
const projectActions = useProjectActions()
const projectRuntime = useProjectRuntime()
/** Project yang sedang difilter, untuk Open project dan state berhenti. */
const focusedProject = computed(() =>
  activeTab.value === 'Projects' && projectFilter.value
    ? useConfig().config.value.projects.find((p) => p.name === projectFilter.value) ?? null
    : null,
)
const focusedInfo = computed(() => (focusedProject.value ? projectRuntime.infoOf(focusedProject.value.id) : null))
const focusedState = computed(() => (focusedProject.value ? projectRuntime.stateOf(focusedProject.value.id) : 'stopped'))

async function copyView(): Promise<void> {
  const text = filteredLines.value.map((l) => `${l.ts} [${l.src}] ${l.text}`).join('\n')
  try {
    await navigator.clipboard.writeText(text)
    toast.show(`Copied ${filteredLines.value.length} line${filteredLines.value.length === 1 ? '' : 's'}`)
  } catch {
    toast.show('Could not copy. Select the lines and press Ctrl+C.', { tone: 'info' })
  }
}
const problemsOnly = ref(false)

const projectNames = computed(() => useConfig().config.value.projects.map((p) => p.name))

function isProblem(l: { level?: string }): boolean {
  return l.level === 'error' || l.level === 'warn'
}

/** Chip per project: nama + jumlah baris error, supaya terlihat siapa yang bermasalah. */
const projectChips = computed(() =>
  useConfig()
    .config.value.projects.map((p) => ({
      name: p.name,
      runnable: !!p.command,
      total: lines.value.filter((l) => l.src === p.name).length,
      errors: lines.value.filter((l) => l.src === p.name && l.level === 'error').length,
    }))
    // Project tanpa perintah tidak pernah punya output; jangan jadi chip kosong.
    .filter((c) => c.runnable || c.total > 0),
)

// Project yang difilter dihapus/diganti nama → kembali ke semua.
watch(projectNames, (names) => {
  if (projectFilter.value && !names.includes(projectFilter.value)) projectFilter.value = ''
})

// `composer run dev` / `concurrently` memberi prefiks `[server]`,
// `[queue]`, `[vite]` pada tiap baris. Bila satu project punya ≥ 2 prefiks,
// tampil sebagai chip kedua supaya satu proses bisa dilihat sendiri.
const PART_PREFIX = /^\[([A-Za-z0-9_.:-]{1,24})\](?:\s|$)/
function partOf(text: string): string {
  return PART_PREFIX.exec(text)?.[1] ?? ''
}
const scopedProject = computed(() => {
  if (activeTab.value !== 'Projects') return ''
  if (projectFilter.value) return projectFilter.value
  return projectChips.value.length === 1 ? projectChips.value[0].name : ''
})
const partFilter = ref('')
const partChips = computed(() => {
  const name = scopedProject.value
  if (!name) return []
  const counts = new Map<string, { total: number; errors: number }>()
  for (const l of lines.value) {
    if (l.src !== name) continue
    const part = partOf(l.text)
    if (!part) continue
    const c = counts.get(part) ?? { total: 0, errors: 0 }
    c.total++
    if (l.level === 'error') c.errors++
    counts.set(part, c)
  }
  return counts.size >= 2 ? [...counts].map(([part, c]) => ({ part, ...c })) : []
})
watch(scopedProject, () => (partFilter.value = ''))
watch(partChips, (chips) => {
  if (partFilter.value && !chips.some((c) => c.part === partFilter.value)) partFilter.value = ''
})

const tabLines = computed(() => {
  if (activeTab.value === 'All') return lines.value
  const tab = activeTab.value
  // Output dev server memakai nama project sebagai sumber.
  if (tab === 'Projects') {
    if (scopedProject.value && partFilter.value) {
      return lines.value.filter((l) => l.src === scopedProject.value && partOf(l.text) === partFilter.value)
    }
    if (projectFilter.value) return lines.value.filter((l) => l.src === projectFilter.value)
    const names = new Set(projectNames.value)
    return lines.value.filter((l) => names.has(l.src))
  }
  const srcSet = TAB_SRC[tab] ?? []
  const keywords = TAB_KEYWORDS[tab] ?? []
  return lines.value.filter((l) => {
    if (l.src && srcSet.includes(l.src.toUpperCase())) return true
    const textLower = l.text.toLowerCase()
    return keywords.some((kw) => textLower.includes(kw))
  })
})

// Filter teks: tiap kata harus ada di teks atau sumber baris.
const textFilter = ref('')
const filterEl = ref<InstanceType<typeof UiInput> | null>(null)
const filterWords = computed(() => textFilter.value.toLowerCase().split(/\s+/).filter(Boolean))

const filteredLines = computed(() => {
  let out = problemsOnly.value ? tabLines.value.filter(isProblem) : tabLines.value
  const words = filterWords.value
  if (words.length > 0) {
    out = out.filter((l) => {
      const hay = `${l.src} ${l.text}`.toLowerCase()
      return words.every((w) => hay.includes(w))
    })
  }
  return out
})

// Hanya baris terbaru yang dirender (buffer bisa sampai 4000 baris);
// "Show older lines" menambah jatah. Jatah kembali ke awal saat tab atau
// filter berubah.
const RENDER_STEP = 1000
const renderLimit = ref(RENDER_STEP)
watch([activeTab, projectFilter, problemsOnly, textFilter], () => (renderLimit.value = RENDER_STEP))
const hiddenCount = computed(() => Math.max(0, filteredLines.value.length - renderLimit.value))
const visibleLines = computed(() => (hiddenCount.value ? filteredLines.value.slice(hiddenCount.value) : filteredLines.value))

// Kata filter disorot di teks baris.
interface Seg { t: string; m: boolean }
function segments(text: string): Seg[] {
  const words = filterWords.value
  if (!words.length) return [{ t: text, m: false }]
  const lower = text.toLowerCase()
  const marks = new Array<boolean>(text.length).fill(false)
  for (const w of words) {
    let i = lower.indexOf(w)
    while (i !== -1) {
      for (let k = i; k < i + w.length; k++) marks[k] = true
      i = lower.indexOf(w, i + w.length)
    }
  }
  const out: Seg[] = []
  for (let i = 0; i < text.length; i++) {
    const last = out[out.length - 1]
    if (last && last.m === marks[i]) last.t += text[i]
    else out.push({ t: text[i], m: marks[i] })
  }
  return out.length ? out : [{ t: text, m: false }]
}

function showOlder(): void {
  const area = logAreaRef.value
  const before = area ? area.scrollHeight - area.scrollTop : 0
  renderLimit.value += RENDER_STEP
  // Pertahankan posisi baca: baris yang tadi terlihat tetap di tempatnya.
  void nextTick(() => {
    if (area) area.scrollTop = area.scrollHeight - before
  })
}

// Ctrl F memfokuskan filter (cari bawaan webview dimatikan di webviewGuards).
function handleFindKey(e: KeyboardEvent): void {
  if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    filterEl.value?.focus()
    filterEl.value?.select()
  }
}

const errorCount = computed(
  () => filteredLines.value.filter((l) => l.level === 'error').length,
)

let areaObserver: ResizeObserver | null = null

function scrollToBottom(): void {
  if (logAreaRef.value) {
    logAreaRef.value.scrollTop = logAreaRef.value.scrollHeight
  }
}

const { handleKeydown, handleCopy, focusContainer } = useLogCopy(logAreaRef, filteredLines)

onMounted(async () => {
  // Dibuka dari "View log" di kartu project: langsung ke output project itu.
  const wanted = useLogFocus().take()
  if (wanted && projectNames.value.includes(wanted)) {
    activeTab.value = 'Projects'
    projectFilter.value = wanted
  }
  document.addEventListener('keydown', handleFindKey)
  document.addEventListener('keydown', handleKeydown)
  logAreaRef.value?.addEventListener('copy', handleCopy)
  await nextTick()
  scrollToBottom()
  // Area log mengecil saat banner update muncul atau jendela diperkecil:
  // tetap tampilkan baris terbaru bila auto-scroll aktif.
  if (logAreaRef.value) {
    areaObserver = new ResizeObserver(() => {
      if (autoScroll.value) scrollToBottom()
    })
    areaObserver.observe(logAreaRef.value)
  }
})

onUnmounted(() => {
  areaObserver?.disconnect()
  document.removeEventListener('keydown', handleFindKey)
  document.removeEventListener('keydown', handleKeydown)
  logAreaRef.value?.removeEventListener('copy', handleCopy)
})

watch(
  () => [lines.value.length, activeTab.value, projectFilter.value, problemsOnly.value, textFilter.value],
  async () => {
    if (autoScroll.value) {
      await nextTick()
      scrollToBottom()
    }
  },
)

async function exportLogs(): Promise<void> {
  const content = filteredLines.value
    .map((l) => `${l.ts} [${l.src}] ${l.text}`)
    .join('\n')

  triggerDownload(content, 'kova-logs.txt')
  toast.show('Saved kova-logs.txt to Downloads')
}

function triggerDownload(text: string, filename: string): void {
  const blob = new Blob([text], { type: 'text/plain;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}
</script>

<template>
  <div class="view view-logs-viewer">
    <!-- Sub-header bar -->
    <header class="lv-header">
      <div class="lv-header__left">
        <h1 class="lv-title">Logs</h1>
      </div>
      <UiTabs
        class="lv-tabs"
        label="Log source"
        :model-value="activeTab"
        :tabs="TABS.map((t) => ({ value: t, label: t }))"
        @update:model-value="activeTab = $event as Tab"
      />
      <div class="lv-header__right">
        <div class="lv-search">
        <UiIcon name="search" class="lv-search__i" />
        <UiInput
          ref="filterEl"
          v-model="textFilter"
          type="search"
          width="260px"
          placeholder="Filter lines"
          aria-label="Filter log lines by text"
          @keydown.esc="textFilter = ''"
        />
        </div>
        <!-- Level sebagai pilihan, bukan tombol tekan. -->
        <UiSegmented
          label="Log level"
          :model-value="problemsOnly ? 'problems' : 'all'"
          :options="[{ value: 'all', label: 'All' }, { value: 'problems', label: 'Errors & warnings' }]"
          @update:model-value="problemsOnly = $event === 'problems'"
        />
        <span class="lv-sp"></span>
        <UiButton v-if="focusedProject" @click="projectActions.openProject(focusedProject)">Open project</UiButton>
        <label class="lv-follow" title="Keep the newest line in view">
          <UiToggle size="sm" label="Follow" :on="autoScroll" @click="autoScroll = !autoScroll" />
          <span aria-hidden="true">Follow</span>
        </label>
        <UiButton :disabled="filteredLines.length === 0" @click="copyView">Copy</UiButton>
        <UiButton variant="quiet" @click="clear">Clear</UiButton>
      </div>
    </header>

    <!-- Filter per project -->
    <div v-if="activeTab === 'Projects' && projectChips.length > 1" class="lv-chips">
      <button
        class="lv-chip"
        :class="{ 'lv-chip--active': projectFilter === '' }"
        :aria-pressed="projectFilter === ''"
        @click="projectFilter = ''"
      >
        All projects
      </button>
      <button
        v-for="c in projectChips"
        :key="c.name"
        class="lv-chip"
        :class="{ 'lv-chip--active': projectFilter === c.name }"
        :aria-pressed="projectFilter === c.name"
        @click="projectFilter = c.name"
      >
        {{ c.name }}<span v-if="c.errors > 0" class="lv-chip__err">{{ c.errors }}</span>
      </button>
    </div>

    <!-- Proses di dalam satu project (prefiks concurrently). -->
    <div v-if="partChips.length > 0" class="lv-chips lv-chips--parts" role="group" :aria-label="`Processes in ${scopedProject}`">
      <button
        class="lv-chip"
        :class="{ 'lv-chip--active': partFilter === '' }"
        :aria-pressed="partFilter === ''"
        @click="partFilter = ''"
      >
        All processes
      </button>
      <button
        v-for="c in partChips"
        :key="c.part"
        class="lv-chip"
        :class="{ 'lv-chip--active': partFilter === c.part }"
        :aria-pressed="partFilter === c.part"
        @click="partFilter = c.part"
      >
        {{ c.part }}<span v-if="c.errors > 0" class="lv-chip__err">{{ c.errors }}</span>
      </button>
    </div>

    <!-- Project berhenti: baris sampai saat berhenti tetap ditampilkan. -->
    <UiBanner
      v-if="focusedProject && focusedState === 'exited' && tabLines.length > 0"
      class="lv-stopped"
      tone="err"
      :title="`${focusedProject.name} stopped${focusedInfo?.exitCode != null ? ` with exit code ${focusedInfo.exitCode}` : ''}`"
    >
      Its output up to the stop is kept below.
      <template #actions>
        <UiButton variant="accent" @click="projectActions.start(focusedProject)">Start {{ focusedProject.name }}</UiButton>
      </template>
    </UiBanner>

    <!-- Log area -->
    <div ref="logAreaRef" class="lv-area" tabindex="0" @click="focusContainer">
      <button v-if="hiddenCount > 0" type="button" class="lv-older" @click.stop="showOlder">
        Show {{ Math.min(hiddenCount, RENDER_STEP).toLocaleString() }} older lines
        <span class="lv-older__count">({{ hiddenCount.toLocaleString() }} hidden)</span>
      </button>
      <div
        v-for="(line, i) in visibleLines"
        :key="hiddenCount + i"
        :data-log-idx="hiddenCount + i"
        class="lv-line"
        :class="{ 'lv-line--error': line.level === 'error', 'lv-line--warn': line.level === 'warn' }"
      >
        <span class="lv-ts">{{ line.ts }}</span>
        <span
          class="lv-badge"
          :style="{
            color: badgeStyle(line.src, line.level).color,
            borderColor: badgeStyle(line.src, line.level).border,
            background: badgeStyle(line.src, line.level).background,
          }"
        >{{ line.src }}</span>
        <span class="lv-text"><template v-for="(g, k) in segments(line.text)" :key="k"><mark v-if="g.m" class="lv-mark">{{ g.t }}</mark><template v-else>{{ g.t }}</template></template></span>
      </div>
      <EmptyState
        v-if="tabLines.length === 0 && focusedProject"
        :title="`No output from ${focusedProject.name} yet`"
        icon="lines"
      >
        {{ focusedState === 'stopped' || focusedState === 'exited' ? 'It is stopped. Start it and its output appears here as it runs.' : 'It is running but has not printed anything yet.' }}
        <template v-if="focusedState === 'stopped' || focusedState === 'exited'" #actions>
          <UiButton variant="accent" @click="projectActions.start(focusedProject)">Start {{ focusedProject.name }}</UiButton>
        </template>
      </EmptyState>
      <EmptyState v-else-if="tabLines.length === 0" title="No output yet" icon="lines">
        Lines appear here when a service or project writes to its log.
      </EmptyState>
      <EmptyState v-else-if="filteredLines.length === 0" title="No lines match" icon="search">
        Nothing in this view matches the current filter.
        <template #actions>
          <UiButton @click="textFilter = ''; problemsOnly = false">Clear filter</UiButton>
        </template>
      </EmptyState>
    </div>

    <!-- Bottom status bar -->
    <footer class="lv-footer">
      <span class="lv-footer__stats">
        {{ filteredLines.length }}<template v-if="filterWords.length > 0 || problemsOnly"> of {{ tabLines.length }}</template> lines
        <span v-if="errorCount > 0" class="lv-footer__errors">&nbsp;· {{ errorCount }} errors</span>
        <span v-else class="lv-footer__ok">&nbsp;· 0 errors</span>
      </span>
      <span class="lv-footer__right">
        <span class="lv-footer__cap">Last 500 lines per source</span>
        <UiButton @click="exportLogs">Export logs</UiButton>
      </span>
    </footer>
  </div>
</template>

<style scoped>
.view-logs-viewer {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-sunken);
  color: var(--text);
  overflow: hidden;
}

/* Sub-header */
.lv-header {
  /* Judul, lalu tab sumber, lalu baris filter. */
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  padding: var(--space-5) var(--space-5) var(--space-3);
  min-height: 40px;
  background: var(--bg);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  gap: var(--space-2) var(--space-4);
}

.lv-header__left {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-shrink: 0;
}

.lv-title {
  margin: 0;
  font-family: var(--font-sans);
  font-size: var(--text-xl);
  font-weight: 600;
  letter-spacing: -0.015em;
  color: var(--text);
}
.lv-header__left { flex-basis: 100%; }
.lv-sp { flex: 1; }
.lv-search { position: relative; }
.lv-search__i { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--muted); pointer-events: none; z-index: 1; }
.lv-search :deep(input) { padding-left: 32px; }

.lv-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 6px var(--space-4);
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

/* Baris kedua: proses di dalam project terpilih, sedikit menjorok. */
.lv-chips--parts { padding-left: calc(var(--space-4) + var(--space-3)); }

.lv-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  padding: 2px 10px;
  border-radius: 999px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}

.lv-chip:hover {
  color: var(--text);
}

.lv-chip--active {
  color: var(--accent);
  border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  background: color-mix(in srgb, var(--accent) 10%, transparent);
}

.lv-chip__err {
  color: var(--red);
  font-weight: 700;
}

.lv-chip:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.lv-tabs { flex: 1 1 100%; }

.lv-header__right {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  flex: 1 1 100%;
}

/* Log area */
.lv-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  padding: var(--space-3) var(--space-4);
  min-height: 0;
  user-select: text;
  outline: none;
}

.lv-area:focus-visible {
  box-shadow: inset 0 0 0 2px var(--accent);
}

.lv-line {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  line-height: 1.65;
  color: var(--text);
  word-break: break-all;
  user-select: text;
}

.lv-line--error .lv-text {
  color: color-mix(in srgb, var(--red) 80%, var(--text));
}

.lv-line--warn .lv-text {
  color: color-mix(in srgb, var(--amber) 80%, var(--text));
}

.lv-ts {
  color: var(--faint);
  flex-shrink: 0;
  min-width: 56px;
}

.lv-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border: 1px solid;
  border-radius: var(--radius-xs);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.02em;
  flex-shrink: 0;
  line-height: 1.4;
}

.lv-mark {
  background: color-mix(in srgb, var(--amber) 35%, transparent);
  color: inherit;
  border-radius: 2px;
}

.lv-older {
  align-self: center;
  margin: 0 0 var(--space-2);
  padding: 2px 10px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: transparent;
  color: var(--muted);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
}
.lv-older:hover { color: var(--text); }
.lv-older:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.lv-older__count { color: var(--faint); }

.lv-text {
  flex: 1;
  user-select: text;
}

/* Footer */
.lv-follow {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--muted);
  cursor: pointer;
  white-space: nowrap;
}
.lv-footer__right { display: inline-flex; align-items: center; gap: var(--space-3); }
.lv-footer__cap { font-size: var(--text-xs); color: var(--muted); }

.lv-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--space-4);
  height: 36px;
  background: var(--surface);
  border-top: 1px solid var(--border);
  flex-shrink: 0;
}

.lv-footer__stats {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--muted);
}

.lv-footer__errors {
  color: var(--red);
}

.lv-footer__ok {
  color: var(--faint);
}
.lv-stopped { margin: var(--space-2) var(--space-6) 0; }
</style>
