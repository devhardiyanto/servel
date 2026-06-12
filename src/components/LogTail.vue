<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import type { LogLine } from '@/composables/useLogs'

const props = withDefaults(defineProps<{
  lines: LogLine[]
  maxHeight?: string
}>(), {
  maxHeight: '200px',
})

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

function tagClass(src?: string, level?: 'warn' | 'error'): string {
  if (level === 'error') return 'log-tag log-tag--error'
  if (level === 'warn') return 'log-tag log-tag--warn'
  if (src === 'UP') return 'log-tag log-tag--up'
  if (src === 'DOWN') return 'log-tag log-tag--down'
  if (src === 'ENV') return 'log-tag log-tag--env'
  if (src === 'PHP' || src === 'PHPVM') return 'log-tag log-tag--php'
  if (src === 'NODE' || src === 'FNM') return 'log-tag log-tag--node'
  if (src === 'DOCKER') return 'log-tag log-tag--docker'
  if (src === 'SERVEL') return 'log-tag log-tag--servel'
  return 'log-tag log-tag--default'
}
</script>

<template>
  <div
    ref="containerRef"
    class="log-tail-body"
    :style="{ maxHeight: maxHeight }"
  >
    <div
      v-for="(line, i) in lines"
      :key="i"
      class="log-line"
    >
      <span class="log-ts">{{ line.ts }}</span>
      <span :class="tagClass(line.src, line.level)">[{{ line.src }}]</span>
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

.log-ts {
  color: var(--dim);
  flex-shrink: 0;
}

.log-tag {
  color: var(--muted);
  flex-shrink: 0;
}

.log-tag--up {
  color: var(--green);
}

.log-tag--down {
  color: var(--amber);
}

.log-tag--env {
  color: var(--accent);
}

.log-tag--error {
  color: var(--red);
}

.log-tag--warn {
  color: var(--amber);
}

.log-tag--php {
  color: #818cf8;
}

.log-tag--node {
  color: var(--green);
}

.log-tag--docker {
  color: #60a5fa;
}

.log-tag--servel {
  color: var(--accent);
}

.log-tag--default {
  color: var(--dim);
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
