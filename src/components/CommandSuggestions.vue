<script setup lang="ts">
// v3.11.0: pilihan perintah di bawah field Command — hasil deteksi, lalu
// script dari composer.json dan package.json (mis. `composer run dev`).
// Klik = isi field; field tetap bisa diketik bebas.
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface ScriptOption {
  label: string
  command: string
  source: 'detected' | 'composer.json' | 'package.json' | string
  note: string | null
}

const props = defineProps<{ path: string; modelValue: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const options = ref<ScriptOption[]>([])

async function load(path: string): Promise<void> {
  options.value = []
  if (!path.trim()) return
  try {
    options.value = await invoke<ScriptOption[]>('project_scripts', { path })
  } catch {
    options.value = []
  }
}
watch(() => props.path, load, { immediate: true })

const current = computed(() => options.value.find((o) => o.command === props.modelValue.trim()) ?? null)
// Satu pilihan = hanya hasil deteksi yang sudah terisi; tidak perlu ditampilkan.
const visible = computed(() => options.value.length > 1)
</script>

<template>
  <div v-if="visible" class="cs" role="group" aria-label="Command suggestions">
    <span class="cs-label">Suggestions</span>
    <button
      v-for="o in options"
      :key="o.command"
      type="button"
      class="cs-opt"
      :class="{ 'cs-opt--on': current?.command === o.command }"
      :aria-pressed="current?.command === o.command"
      :title="o.source === 'detected' ? 'Detected from the project files' : `Script from ${o.source}`"
      @click="emit('update:modelValue', o.command)"
    >
      <span class="cs-cmd">{{ o.label }}</span>
      <span class="cs-src">{{ o.source === 'detected' ? 'detected' : o.source }}</span>
    </button>
    <p v-if="current?.note" class="cs-note" role="note">{{ current.note }}</p>
  </div>
</template>

<style scoped>
.cs {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-top: calc(-1 * var(--space-1));
}
.cs-label {
  flex-basis: 100%;
  font-size: var(--text-xs);
  color: var(--faint);
}
.cs-opt {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 26px;
  padding: 0 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-xs);
  background: var(--surface2);
  color: var(--text);
  font: inherit;
  text-align: left;
  max-width: 100%;
  cursor: pointer;
}
.cs-opt:hover { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
.cs-opt:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.cs-opt--on {
  border-color: color-mix(in srgb, var(--accent) 60%, var(--border));
  background: color-mix(in srgb, var(--accent) 12%, var(--surface2));
}
.cs-cmd { font-family: var(--font-mono); font-size: var(--text-xs); overflow-wrap: anywhere; }
.cs-src { font-size: var(--text-xs); color: var(--faint); }
.cs-note {
  flex-basis: 100%;
  margin: 2px 0 0;
  font-size: var(--text-xs);
  color: var(--amber);
}
</style>
