<script setup lang="ts">
// Tab bergaris bawah. Panah kiri/kanan berpindah tab; Home/End ke ujung.
import { ref } from 'vue'

const props = defineProps<{
  modelValue: string
  tabs: { value: string; label: string; badge?: string | number; alert?: boolean }[]
  label: string
}>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const root = ref<HTMLElement | null>(null)

function go(index: number): void {
  const n = props.tabs.length
  const tab = props.tabs[(index + n) % n]
  if (!tab) return
  emit('update:modelValue', tab.value)
  root.value?.querySelectorAll<HTMLButtonElement>('button')[(index + n) % n]?.focus()
}
function current(): number {
  return props.tabs.findIndex((t) => t.value === props.modelValue)
}
</script>

<template>
  <div ref="root" class="ui-tabs" role="tablist" :aria-label="label">
    <button
      v-for="t in tabs"
      :key="t.value"
      type="button"
      role="tab"
      class="ui-tab"
      :class="{ 'ui-tab--on': t.value === modelValue }"
      :aria-selected="t.value === modelValue"
      :tabindex="t.value === modelValue ? 0 : -1"
      @click="emit('update:modelValue', t.value)"
      @keydown.right.prevent="go(current() + 1)"
      @keydown.left.prevent="go(current() - 1)"
      @keydown.home.prevent="go(0)"
      @keydown.end.prevent="go(tabs.length - 1)"
    >
      {{ t.label }}
      <span
        v-if="t.badge"
        class="ui-tab__badge"
        :class="{ 'ui-tab__badge--alert': t.alert }"
        :aria-label="t.alert ? `${t.badge} errors` : undefined"
      >{{ t.badge }}</span>
    </button>
  </div>
</template>

<style scoped>
.ui-tabs { display: flex; flex-wrap: wrap; gap: 0 18px; min-width: 0; }
.ui-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 36px;
  padding: 0;
  border: 0;
  border-bottom: 2px solid transparent;
  background: none;
  color: var(--muted);
  font: 500 var(--text-sm) var(--font-sans);
  cursor: pointer;
  transition: color var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease);
}
.ui-tab:hover { color: var(--text); }
.ui-tab--on { color: var(--text); border-bottom-color: var(--accent); }
.ui-tab:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.ui-tab__badge {
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-xs);
  background: var(--surface3);
  color: var(--text);
  font: 500 var(--text-xs) var(--font-mono);
}
.ui-tab__badge--alert { background: color-mix(in srgb, var(--red) 16%, transparent); color: var(--red); }
</style>
