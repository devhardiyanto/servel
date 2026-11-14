<script setup lang="ts">
import { ref, toRef, watch, nextTick, onMounted, onUnmounted } from 'vue'
import type { LogLine } from '@/composables/useLogs'
import { useLogCopy } from '@/composables/useLogCopy'
import { badgeStyle } from '@/utils/logColors'

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
  background: var(--bg-sunken);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
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
  border-radius: var(--radius-xs);
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
