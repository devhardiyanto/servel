<script setup lang="ts">
// Pilihan tunggal dari beberapa opsi (radiogroup). Panah kiri/kanan memindah
// pilihan; hanya opsi aktif yang ada di urutan Tab (roving tabindex).
import { ref } from 'vue'

const props = defineProps<{
  modelValue: string
  options: { value: string; label: string; disabled?: boolean }[]
  label: string
}>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const root = ref<HTMLElement | null>(null)

function move(delta: number): void {
  const enabled = props.options.filter((o) => !o.disabled)
  const i = enabled.findIndex((o) => o.value === props.modelValue)
  const next = enabled[(i + delta + enabled.length) % enabled.length]
  if (!next) return
  emit('update:modelValue', next.value)
  const idx = props.options.indexOf(next)
  root.value?.querySelectorAll<HTMLButtonElement>('button')[idx]?.focus()
}
</script>

<template>
  <div ref="root" class="ui-seg" role="radiogroup" :aria-label="label">
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      role="radio"
      :class="{ 'ui-seg__on': o.value === modelValue }"
      :aria-checked="o.value === modelValue"
      :tabindex="o.value === modelValue ? 0 : -1"
      :disabled="o.disabled"
      @click="emit('update:modelValue', o.value)"
      @keydown.right.prevent="move(1)"
      @keydown.down.prevent="move(1)"
      @keydown.left.prevent="move(-1)"
      @keydown.up.prevent="move(-1)"
    >{{ o.label }}</button>
  </div>
</template>

<style scoped>
.ui-seg {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg);
}
.ui-seg button {
  height: 26px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--radius-xs);
  background: transparent;
  color: var(--muted);
  font: 500 var(--text-xs) var(--font-sans);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.ui-seg button:hover:not(:disabled) { color: var(--text); }
.ui-seg button:disabled { opacity: 0.5; cursor: not-allowed; }
.ui-seg .ui-seg__on { background: var(--surface2); color: var(--text); box-shadow: 0 0 0 1px var(--border-strong); }
.ui-seg button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
</style>
