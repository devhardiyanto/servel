<script setup lang="ts">
import { ref, computed, watch, nextTick, inject, onMounted, onUnmounted } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { useLogs } from '@/composables/useLogs'
import { useLogCopy } from '@/composables/useLogCopy'
import { SetViewKey } from '@/types/navigation'

const setView = inject(SetViewKey)!
const { lines, clear } = useLogs()

const TABS = ['All', 'MySQL', 'PostgreSQL', 'Redis', 'phpvm', 'fnm'] as const
type Tab = (typeof TABS)[number]

const activeTab = ref<Tab>('All')
const autoScroll = ref(true)
const logAreaRef = ref<HTMLElement | null>(null)

const TAB_SRC: Record<string, string[]> = {
  MySQL: ['MYSQL'],
  PostgreSQL: ['POSTGRES', 'PGSQL'],
  Redis: ['REDIS'],
  phpvm: ['PHP', 'PHPVM'],
  fnm: ['NODE', 'FNM'],
}

const TAB_KEYWORDS: Record<string, string[]> = {
  MySQL: ['mysql', 'servel_mysql'],
  PostgreSQL: ['postgres', 'servel_postgres'],
  Redis: ['redis', 'servel_redis'],
  phpvm: ['phpvm', 'php '],
  fnm: ['fnm', 'node '],
}

const filteredLines = computed(() => {
  if (activeTab.value === 'All') return lines.value
  const tab = activeTab.value
  const srcSet = TAB_SRC[tab] ?? []
  const keywords = TAB_KEYWORDS[tab] ?? []
  return lines.value.filter((l) => {
    if (l.src && srcSet.includes(l.src.toUpperCase())) return true
    const textLower = l.text.toLowerCase()
    return keywords.some((kw) => textLower.includes(kw))
  })
})

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
  () => lines.value.length,
  async () => {
    if (autoScroll.value) {
      await nextTick()
      scrollToBottom()
    }
  },
)

interface BadgeStyle {
  color: string
  border: string
  background: string
}

const SOURCE_COLORS: Record<string, string> = {
  SERVEL: '#22d3ee',
  ENV: '#22d3ee',
  DOCKER: '#60a5fa',
  MYSQL: '#fb923c',
  POSTGRES: '#60a5fa',
  PGSQL: '#60a5fa',
  REDIS: '#f87171',
  MAILPIT: '#c084fc',
  PHPVM: '#818cf8',
  PHP: '#818cf8',
  FNM: '#4ade80',
  NODE: '#4ade80',
  RABBITMQ: '#fb923c',
  MONGODB: '#10b981',
  MINIO: '#f59e0b',
  GOTENBERG: '#a78bfa',
  SQLSERVER: '#ef4444',
  UP: '#4ade80',
  DOWN: '#fbbf24',
}

function badgeStyle(src?: string, level?: 'warn' | 'error'): BadgeStyle {
  const errColor = '#f87171'
  const warnColor = '#fbbf24'
  if (level === 'error') {
    return { color: errColor, border: errColor + '33', background: errColor + '0d' }
  }
  if (level === 'warn') {
    return { color: warnColor, border: warnColor + '33', background: warnColor + '0d' }
  }
  const c = (src && SOURCE_COLORS[src.toUpperCase()]) || '#a1a1aa'
  return { color: c, border: c + '33', background: c + '0d' }
}

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
        no logs — output will appear here
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
  background: #09090b;
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
  border-radius: 3px;
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
  border-radius: 4px;
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
  border-radius: 3px;
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
