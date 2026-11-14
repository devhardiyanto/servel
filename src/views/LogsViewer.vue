<script setup lang="ts">
import { ref, computed, watch, nextTick, inject, onMounted, onUnmounted } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { useLogs } from '@/composables/useLogs'
import { useLogCopy } from '@/composables/useLogCopy'
import { badgeStyle } from '@/utils/logColors'
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'

const setView = inject(SetViewKey)!
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
  PHP: ['PHP', 'PHPVM'],
  Node: ['NODE', 'FNM'],
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

const filteredLines = computed(() => (problemsOnly.value ? tabLines.value.filter(isProblem) : tabLines.value))

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
  document.addEventListener('keydown', handleKeydown)
  logAreaRef.value?.addEventListener('copy', handleCopy)
  await nextTick()
  scrollToBottom()
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown)
  logAreaRef.value?.removeEventListener('copy', handleCopy)
})

watch(
  () => [lines.value.length, activeTab.value, projectFilter.value, problemsOnly.value],
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
        <button class="lv-back-btn" @click="setView('dashboard')">&#8592;</button>
        <span class="lv-title">LOGS</span>
      </div>
      <nav class="lv-tabs">
        <button
          v-for="tab in TABS"
          :key="tab"
          class="lv-tab"
          :class="{ 'lv-tab--active': activeTab === tab }"
          @click="activeTab = tab"
        >{{ tab }}</button>
      </nav>
      <div class="lv-header__right">
        <button
          class="lv-btn lv-btn--outline"
          :class="{ 'lv-btn--problems-on': problemsOnly }"
          :aria-pressed="problemsOnly"
          title="Show only lines marked as error or warning"
          @click="problemsOnly = !problemsOnly"
        >Errors &amp; warnings</button>
        <button class="lv-btn lv-btn--outline" @click="clear">Clear</button>
        <button
          class="lv-btn lv-btn--autoscroll"
          :class="{ 'lv-btn--autoscroll-on': autoScroll }"
          @click="autoScroll = !autoScroll"
        >
          &#8595; Auto-scroll {{ autoScroll ? 'ON' : 'OFF' }}
        </button>
      </div>
    </header>

    <!-- Filter per project -->
    <div v-if="activeTab === 'Projects' && projectChips.length > 1" class="lv-chips">
      <button class="lv-chip" :class="{ 'lv-chip--active': projectFilter === '' }" @click="projectFilter = ''">
        All projects
      </button>
      <button
        v-for="c in projectChips"
        :key="c.name"
        class="lv-chip"
        :class="{ 'lv-chip--active': projectFilter === c.name }"
        @click="projectFilter = c.name"
      >
        {{ c.name }}<span v-if="c.errors > 0" class="lv-chip__err">{{ c.errors }}</span>
      </button>
    </div>

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
      <div v-if="filteredLines.length === 0" class="lv-empty">
        {{ problemsOnly && tabLines.length > 0 ? 'no errors or warnings in these logs' : 'no logs — output will appear here' }}
      </div>
    </div>

    <!-- Bottom status bar -->
    <footer class="lv-footer">
      <span class="lv-footer__stats">
        {{ filteredLines.length }} lines
        <span v-if="errorCount > 0" class="lv-footer__errors">&nbsp;· {{ errorCount }} errors</span>
        <span v-else class="lv-footer__ok">&nbsp;· 0 errors</span>
      </span>
      <button class="lv-btn lv-btn--outline" @click="exportLogs">Export Logs</button>
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
  padding: 0 var(--space-4);
  height: 40px;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  gap: var(--space-4);
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
  font-size: 16px;
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
  font-size: 11px;
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
  font-size: 11px;
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

.lv-btn--problems-on {
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 50%, transparent);
  background: color-mix(in srgb, var(--red) 10%, transparent);
}

.lv-chip:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.lv-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: 1;
  justify-content: center;
}

.lv-tab {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 4px 10px;
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--muted);
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s;
  line-height: 1.4;
}

.lv-tab:hover:not(.lv-tab--active) {
  color: var(--text);
}

.lv-tab--active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}

.lv-header__right {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}

.lv-btn {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 3px 10px;
  border-radius: var(--radius-xs);
  cursor: pointer;
  transition: background 0.1s, border-color 0.1s, color 0.1s;
}

.lv-btn--outline {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
}

.lv-btn--outline:hover {
  border-color: var(--dim);
  color: var(--text);
}

.lv-btn--autoscroll {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--dim);
}

.lv-btn--autoscroll-on {
  border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}

.lv-btn--autoscroll:hover {
  border-color: var(--muted);
  color: var(--text);
}

/* Log area */
.lv-area {
  flex: 1;
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
  font-size: 11px;
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
  color: var(--dim);
  flex-shrink: 0;
  min-width: 56px;
}

.lv-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border: 1px solid;
  border-radius: var(--radius-xs);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.02em;
  flex-shrink: 0;
  line-height: 1.4;
}

.lv-text {
  flex: 1;
  user-select: text;
}

.lv-empty {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
  padding: var(--space-3) 0;
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
  font-size: 11px;
  color: var(--muted);
}

.lv-footer__errors {
  color: var(--red);
}

.lv-footer__ok {
  color: var(--dim);
}
</style>
