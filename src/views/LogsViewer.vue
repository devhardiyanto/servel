<script setup lang="ts">
import UiTabs from '@/components/ui/UiTabs.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiInput from '@/components/ui/UiInput.vue'
import { useLogFocus } from '@/composables/useLogFocus'
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
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

const tabLines = computed(() => {
  if (activeTab.value === 'All') return lines.value
  const tab = activeTab.value
  // Output dev server memakai nama project sebagai sumber.
  if (tab === 'Projects') {
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

// Filter teks (v3.2): tiap kata harus ada di teks atau sumber baris.
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
})

onUnmounted(() => {
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

  // Try Tauri dialog plugin first
  let saved = false
  try {
    const filePath = await save({
      defaultPath: 'servel-logs.txt',
      filters: [{ name: 'Logs', extensions: ['txt', 'log'] }],
    })
    if (filePath) {
      // plugin-fs not installed — fall back to blob download
      // The save dialog gives us a path but we can't write without plugin-fs.
      // Trigger browser download as fallback.
      triggerDownload(content, 'servel-logs.txt')
      saved = true
    }
  } catch {
    // dialog plugin unavailable — use blob fallback directly
  }

  if (!saved) {
    triggerDownload(content, 'servel-logs.txt')
  }
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
        <span class="lv-title">LOGS</span>
      </div>
      <UiTabs
        class="lv-tabs"
        label="Log source"
        :model-value="activeTab"
        :tabs="TABS.map((t) => ({ value: t, label: t }))"
        @update:model-value="activeTab = $event as Tab"
      />
      <div class="lv-header__right">
        <UiInput
          ref="filterEl"
          v-model="textFilter"
          type="search"
          width="180px"
          placeholder="Filter lines"
          aria-label="Filter log lines by text"
          @keydown.esc="textFilter = ''"
        />
        <UiButton
          :variant="problemsOnly ? 'accent' : 'default'"
          :aria-pressed="problemsOnly"
          title="Show only lines marked as error or warning"
          @click="problemsOnly = !problemsOnly"
        >Errors &amp; warnings</UiButton>
        <UiButton v-if="focusedProject" @click="projectActions.openProject(focusedProject)">Open project</UiButton>
        <UiButton :disabled="filteredLines.length === 0" @click="copyView">Copy</UiButton>
        <UiButton @click="clear">Clear</UiButton>
        <UiButton
          :variant="autoScroll ? 'accent' : 'default'"
          :aria-pressed="autoScroll"
          title="Keep the newest line in view"
          @click="autoScroll = !autoScroll"
        >Auto-scroll</UiButton>
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
      <div
        v-for="(line, i) in filteredLines"
        :key="i"
        :data-log-idx="i"
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
        <span class="lv-text">{{ line.text }}</span>
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
      <UiButton @click="exportLogs">Export logs</UiButton>
    </footer>
  </div>
</template>

<style scoped>
.view-logs-viewer {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--bg-sunken);
  color: var(--text);
  overflow: hidden;
}

/* Sub-header */
.lv-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  padding: var(--space-1) var(--space-4);
  min-height: 40px;
  background: var(--surface);
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

.lv-back-btn {
  background: transparent;
  border: none;
  color: var(--muted);
  font-size: var(--text-lg);
  cursor: pointer;
  padding: 2px 4px;
  line-height: 1;
  border-radius: var(--radius-xs);
  transition: color 0.1s;
}

.lv-back-btn:hover {
  color: var(--text);
}

.lv-title {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--muted);
  text-transform: uppercase;
}

.lv-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 6px var(--space-4);
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

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

.lv-tabs { flex: 1 1 auto; }

.lv-header__right {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
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
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent);
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

.lv-text {
  flex: 1;
  user-select: text;
}

/* Footer */
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
