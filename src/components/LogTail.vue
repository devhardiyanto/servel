<script setup lang="ts">
import { ref, toRef, watch, nextTick, onMounted, onUnmounted } from 'vue'
import type { LogLine } from '@/composables/useLogs'
import { useLogCopy } from '@/composables/useLogCopy'

const props = withDefaults(defineProps<{
  lines: LogLine[]
  maxHeight?: string
}>(), {
  maxHeight: '200px',
})

const containerRef = ref<HTMLElement | null>(null)
const linesRef = toRef(props, 'lines')
const { handleKeydown, handleCopy, focusContainer } = useLogCopy(containerRef, linesRef)

watch(
  () => props.lines.length,
  async () => {
    await nextTick()
    if (containerRef.value) {
      containerRef.value.scrollTop = containerRef.value.scrollHeight
    }
  },
)

onMounted(async () => {
  document.addEventListener('keydown', handleKeydown)
  containerRef.value?.addEventListener('copy', handleCopy)
  await nextTick()
  if (containerRef.value) {
    containerRef.value.scrollTop = containerRef.value.scrollHeight
  }
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown)
  containerRef.value?.removeEventListener('copy', handleCopy)
})

interface BadgeStyle {
  color: string
  border: string
  background: string
}

// Source palette per discussion/servel-logs.jsx SOURCES map.
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
</script>

<template>
  <div
    ref="containerRef"
    class="log-tail-body"
    :style="{ maxHeight: maxHeight }"
    tabindex="0"
    @click="focusContainer"
  >
    <div
      v-for="(line, i) in lines"
      :key="i"
      :data-log-idx="i"
      class="log-line"
      :class="{ 'log-line--warn': line.level === 'warn', 'log-line--error': line.level === 'error' }"
    >
      <span class="log-ts">{{ line.ts }}</span>
      <span
        class="log-badge"
        :style="{
          color: badgeStyle(line.src, line.level).color,
          borderColor: badgeStyle(line.src, line.level).border,
          background: badgeStyle(line.src, line.level).background,
        }"
      >{{ line.src }}</span>
      <span class="log-text">{{ line.text }}</span>
    </div>
    <div v-if="lines.length === 0" class="log-empty">
      no logs &#x2014; command output will appear here
    </div>
  </div>
</template>

<style scoped>
.log-tail-body {
  background: #09090b;
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow-y: auto;
  padding: var(--space-2) var(--space-3);
  height: 100%;
  box-sizing: border-box;
  user-select: text;
  outline: none;
}

.log-tail-body:focus-visible {
  border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
}

.log-line {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.65;
  color: var(--text);
  word-break: break-all;
  user-select: text;
}

.log-line--warn .log-text {
  color: color-mix(in srgb, var(--amber) 80%, var(--text));
}

.log-line--error .log-text {
  color: color-mix(in srgb, var(--red) 80%, var(--text));
}

.log-ts {
  color: var(--dim);
  flex-shrink: 0;
  font-size: 11px;
  min-width: 56px;
}

.log-badge {
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

.log-text {
  flex: 1;
  user-select: text;
}

.log-empty {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--dim);
  padding: var(--space-2) 0;
}
</style>
