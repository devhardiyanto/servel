<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import type { LogLine } from '@/composables/useLogs'

const props = withDefaults(defineProps<{
  lines: LogLine[]
  maxHeight?: string
}>(), {
  maxHeight: '200px',
})

const emit = defineEmits<{
  (e: 'clear'): void
}>()

const containerRef = ref<HTMLElement | null>(null)

watch(
  () => props.lines.length,
  async () => {
    await nextTick()
    if (containerRef.value) {
      containerRef.value.scrollTop = containerRef.value.scrollHeight
    }
  },
)

function levelClass(level?: 'warn' | 'error'): string {
  if (level === 'error') return 'log-level--error'
  if (level === 'warn') return 'log-level--warn'
  return ''
}
</script>

<template>
  <div class="log-tail-wrap">
    <div class="log-tail-header">
      <span class="log-tail-title">Log Output</span>
      <button class="log-tail-clear" @click="emit('clear')">Clear</button>
    </div>
    <div
      ref="containerRef"
      class="log-tail-body"
      :style="{ maxHeight: maxHeight }"
    >
      <div
        v-for="(line, i) in lines"
        :key="i"
        class="log-line"
        :class="levelClass(line.level)"
      >
        <span class="log-ts">{{ line.ts }}</span>
        <span class="log-src">[{{ line.src }}]</span>
        <span class="log-text">{{ line.text }}</span>
      </div>
      <div v-if="lines.length === 0" class="log-empty">
        tidak ada log — output command akan muncul di sini
      </div>
    </div>
  </div>
</template>

<style scoped>
.log-tail-wrap {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}

.log-tail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--border);
  background: var(--surface2);
}

.log-tail-title {
  font-family: var(--font-sans);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--muted);
}

.log-tail-clear {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--dim);
  background: none;
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 2px 8px;
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s;
}

.log-tail-clear:hover {
  color: var(--text);
  border-color: var(--muted);
}

.log-tail-body {
  overflow-y: auto;
  padding: var(--space-2) var(--space-3);
}

.log-line {
  display: flex;
  gap: var(--space-2);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.6;
  color: var(--text);
  word-break: break-all;
}

.log-line.log-level--error {
  color: var(--red);
}

.log-line.log-level--warn {
  color: var(--amber);
}

.log-ts {
  color: var(--dim);
  flex-shrink: 0;
}

.log-src {
  color: var(--muted);
  flex-shrink: 0;
}

.log-text {
  flex: 1;
}

.log-empty {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--dim);
  padding: var(--space-2) 0;
}
</style>
