<script setup lang="ts">
// Kolom teks, textarea, atau select dengan label, bantuan, dan pesan error.
import { computed, ref, useId } from 'vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    label?: string
    help?: string
    error?: string
    mono?: boolean
    readonly?: boolean
    disabled?: boolean
    placeholder?: string
    /** `textarea` memakai `rows`; `select` memakai `options`. */
    as?: 'input' | 'textarea' | 'select'
    rows?: number
    options?: { value: string; label: string }[]
    width?: string
    /** Nama aksesibel bila tidak ada `label` yang terlihat. */
    ariaLabel?: string
    /** Hanya untuk input: `search` menambah tombol hapus bawaan. */
    type?: 'text' | 'search'
    /** Id `<datalist>` untuk saran. */
    list?: string
    maxlength?: number
  }>(),
  { as: 'input', rows: 3, mono: false, readonly: false, disabled: false, type: 'text' },
)
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const id = useId()
const describedBy = computed(() => (props.error || props.help ? `${id}-help` : undefined))

const el = ref<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement | null>(null)
defineExpose({
  focus: () => el.value?.focus(),
  select: () => (el.value as HTMLInputElement | null)?.select?.(),
})

function onInput(e: Event): void {
  emit('update:modelValue', (e.target as HTMLInputElement).value)
}
</script>

<template>
  <div class="ui-field" :style="width ? { width } : undefined">
    <label v-if="label" class="ui-field__label" :for="id">{{ label }}</label>
    <div v-if="as === 'select'" class="ui-field__select">
      <select
        :id="id"
        ref="el"
        :aria-label="ariaLabel"
        class="ui-input"
        :class="{ 'ui-input--err': !!error }"
        :value="modelValue"
        :disabled="disabled"
        :aria-invalid="!!error || undefined"
        :aria-describedby="describedBy"
        @change="onInput"
      >
        <option v-for="o in options" :key="o.value" :value="o.value">{{ o.label }}</option>
      </select>
      <svg class="ui-field__chev" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg>
    </div>
    <textarea
      v-else-if="as === 'textarea'"
      :id="id"
      ref="el"
      :aria-label="ariaLabel"
      class="ui-input ui-input--area"
      :class="{ 'ui-input--mono': mono, 'ui-input--err': !!error }"
      :value="modelValue"
      :rows="rows"
      :placeholder="placeholder"
      :disabled="disabled"
      :readonly="readonly"
      :aria-invalid="!!error || undefined"
      :aria-describedby="describedBy"
      spellcheck="false"
      @input="onInput"
    ></textarea>
    <input
      v-else
      :id="id"
      ref="el"
      :type="type"
      :list="list"
      :maxlength="maxlength"
      :aria-label="ariaLabel"
      class="ui-input"
      :class="{ 'ui-input--mono': mono, 'ui-input--err': !!error, 'ui-input--ro': readonly }"
      :value="modelValue"
      :placeholder="placeholder"
      :disabled="disabled"
      :readonly="readonly"
      :aria-invalid="!!error || undefined"
      :aria-describedby="describedBy"
      spellcheck="false"
      autocomplete="off"
      @input="onInput"
    />
    <span v-if="error || help" :id="`${id}-help`" class="ui-field__help" :class="{ 'ui-field__help--err': !!error }">
      {{ error || help }}
    </span>
  </div>
</template>

<style scoped>
.ui-field { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.ui-field__label { font-size: var(--text-xs); font-weight: 500; color: var(--muted); }
.ui-input {
  height: 32px;
  min-width: 0;
  width: 100%;
  padding: 0 10px;
  border-radius: var(--radius-xs);
  border: 1px solid var(--border-strong);
  background: var(--bg);
  color: var(--text);
  font: 400 var(--text-sm) var(--font-sans);
}
.ui-input::placeholder { color: var(--faint); }
.ui-input--mono { font: 400 var(--text-xs) var(--font-mono); }
.ui-input:focus { outline: 2px solid var(--accent); outline-offset: -1px; border-color: transparent; }
.ui-input--err { border-color: var(--red); }
.ui-input:disabled { opacity: 0.5; }
.ui-input--ro { background: var(--surface2); color: var(--muted); }
.ui-input--area { height: auto; padding: 8px 10px; resize: vertical; line-height: 1.6; }
.ui-field__select { position: relative; display: flex; min-width: 0; }
.ui-field__select .ui-input { appearance: none; -webkit-appearance: none; padding-right: 28px; }
.ui-field__chev {
  position: absolute;
  right: 8px;
  top: 8px;
  width: 16px;
  height: 16px;
  pointer-events: none;
  fill: none;
  stroke: var(--muted);
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.ui-field__help { font-size: var(--text-xs); color: var(--muted); }
.ui-field__help--err { color: var(--red); }
</style>
