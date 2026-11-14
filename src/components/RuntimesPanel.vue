<script setup lang="ts">
// Go dan Python lewat mise (v2.11): versi yang berlaku, versi terpasang,
// install, dan pakai global atau untuk project yang dipilih di "Versions for".
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiButton from '@/components/ui/UiButton.vue'
import { useVersionContext } from '@/composables/useVersionContext'
import type { ToolCurrent } from '@/types/mise'

interface RuntimeVersions {
  tool: string
  installed: { version: string; active: boolean }[]
  current: ToolCurrent | null
}

const TOOLS = [
  { id: 'go', label: 'Go', example: '1.23' },
  { id: 'python', label: 'Python', example: '3.12' },
] as const

const versionCtx = useVersionContext()
const project = computed(() => versionCtx.ctx.value?.project ?? null)

const data = ref<Record<string, RuntimeVersions | null>>({})
const picked = ref<Record<string, string>>({})
const newVersion = ref<Record<string, string>>({})
const busy = ref<string | null>(null)
const error = ref<Record<string, string>>({})
const VERSION_RE = /^\d+(\.\d+){0,2}$/

async function load(tool: string): Promise<void> {
  try {
    const r = await invoke<RuntimeVersions>('runtime_versions', { tool })
    data.value = { ...data.value, [tool]: r }
    if (!picked.value[tool]) picked.value[tool] = r.current?.version ?? r.installed[0]?.version ?? ''
  } catch (err) {
    data.value = { ...data.value, [tool]: null }
    error.value = { ...error.value, [tool]: String(err) }
  }
}

async function run(tool: string, action: () => Promise<void>): Promise<void> {
  busy.value = tool
  error.value = { ...error.value, [tool]: '' }
  try {
    await action()
    await load(tool)
    await versionCtx.refresh()
  } catch (err) {
    error.value = { ...error.value, [tool]: String(err) }
  } finally {
    busy.value = null
  }
}

function install(tool: string): void {
  const version = (newVersion.value[tool] ?? '').trim()
  if (!VERSION_RE.test(version)) {
    error.value = { ...error.value, [tool]: 'Enter a version such as 1.23 or 3.12.4.' }
    return
  }
  void run(tool, async () => {
    await invoke('runtime_install', { tool, version })
    picked.value[tool] = version
    newVersion.value[tool] = ''
  })
}

function use(tool: string, scope: 'global' | 'project'): void {
  const version = picked.value[tool]
  if (!version) return
  void run(tool, () => invoke('runtime_use', { tool, version, scope }))
}

function sourceLabel(t: ToolCurrent | null): string {
  if (!t) return 'not set'
  if (t.scope === 'global') return 'global'
  return t.source?.split(/[\\/]/).pop() ?? 'project'
}

onMounted(() => TOOLS.forEach((t) => void load(t.id)))
</script>

<template>
  <div class="rt">
    <p class="rt-hint">
      Versions come from mise, like PHP and Node.
      <template v-if="project">"Use in {{ project.name }}" writes to that project's mise.toml.</template>
      <template v-else>Pick a project in "Versions for" on the Dashboard to set a version for one project.</template>
    </p>

    <div v-for="t in TOOLS" :key="t.id" class="rt-row">
      <div class="rt-head">
        <span class="rt-name">{{ t.label }}</span>
        <span class="rt-current">
          <b>{{ data[t.id]?.current?.version ?? '—' }}</b>
          <span class="rt-dim">{{ sourceLabel(data[t.id]?.current ?? null) }}</span>
        </span>
      </div>

      <div class="rt-controls">
        <select
          v-model="picked[t.id]"
          class="rt-select"
          :aria-label="`${t.label} version`"
          :disabled="!data[t.id]?.installed.length"
        >
          <option v-if="!data[t.id]?.installed.length" value="">none installed</option>
          <option v-for="v in data[t.id]?.installed ?? []" :key="v.version" :value="v.version">{{ v.version }}</option>
        </select>
        <UiButton :disabled="busy !== null || !picked[t.id]" @click="use(t.id, 'global')">Use globally</UiButton>
        <UiButton v-if="project" variant="accent" :disabled="busy !== null || !picked[t.id]" @click="use(t.id, 'project')">
          Use in {{ project.name }}
        </UiButton>

        <span class="rt-sep"></span>
        <input
          v-model="newVersion[t.id]"
          class="rt-input"
          :placeholder="`e.g. ${t.example}`"
          :aria-label="`${t.label} version to install`"
          spellcheck="false"
          @keydown.enter="install(t.id)"
        />
        <UiButton :loading="busy === t.id" :disabled="busy !== null" @click="install(t.id)">Install</UiButton>
      </div>
      <p v-if="error[t.id]" class="rt-error">{{ error[t.id] }}</p>
    </div>
  </div>
</template>

<style scoped>
.rt-hint { margin: 0 0 var(--space-3); font-size: var(--text-xs); color: var(--muted); }
.rt-row { padding: var(--space-3) 0; border-top: 1px solid var(--border); }
.rt-head { display: flex; align-items: baseline; gap: var(--space-3); margin-bottom: var(--space-2); }
.rt-name { font-size: var(--text-sm); color: var(--text); min-width: 64px; }
.rt-current { display: inline-flex; gap: var(--space-2); font-size: var(--text-xs); }
.rt-current b { font-family: var(--font-mono); font-weight: 500; color: var(--text); }
.rt-dim { color: var(--faint); }
.rt-controls { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); }
.rt-sep { flex: 1; }
.rt-select,
.rt-input {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  padding: 4px 8px;
  background: var(--surface);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-xs);
  color: var(--text);
}
.rt-input { width: 110px; }
.rt-select:focus-visible,
.rt-input:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.rt-error { margin: var(--space-2) 0 0; font-size: var(--text-xs); color: var(--red); }
</style>
