<script setup lang="ts">
// Daftar project (v3.4, canvas Projects): kartu ringkas, dipilih untuk dilihat
// di panel detail. Aksi ada di panel detail, bukan di kartu.
import { computed } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import UiButton from '@/components/ui/UiButton.vue'
import UiChip from '@/components/ui/UiChip.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import StatusDot from '@/components/ui/StatusDot.vue'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime } from '@/composables/useProjectRuntime'
import { useProjectActions } from '@/composables/useProjectActions'
import type { Project } from '@/types/mise'

const props = defineProps<{ selectedId?: string | null; query?: string }>()
const emit = defineEmits<{ select: [id: string] }>()

const { config } = useConfig()
const { stateOf, infoOf } = useProjectRuntime()
/** v3.9.0 (canvas H6): environment proses yang jalan, bila bukan Default. */
function envOf(id: string): string {
  const i = infoOf(id)
  return i && (i.state === 'running' || i.state === 'starting' || i.state === 'unresponsive') ? (i.envName ?? '') : ''
}
const actions = useProjectActions()

const all = computed<Project[]>(() => config.value.projects)
const shown = computed<Project[]>(() => {
  const words = (props.query ?? '').toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return all.value
  return all.value.filter((p) => {
    const hay = `${p.name} ${p.path} ${p.kind ?? ''} ${p.port ?? ''}`.toLowerCase()
    return words.every((w) => hay.includes(w))
  })
})

async function howDetection(): Promise<void> {
  const url = 'https://github.com/devhardiyanto/servel#projects'
  try {
    await openUrl(url)
  } catch {
    window.open(url, '_blank', 'noopener')
  }
}
</script>

<template>
  <EmptyState v-if="all.length === 0" title="No projects yet" icon="folder">
    Add a project folder. Kova detects Laravel, Django, Nuxt, Vite, Next, Go and more, then runs it on a fixed localhost port.
    <template #actions>
      <UiButton variant="primary" size="md" @click="actions.adding.value = true">Add project</UiButton>
      <UiButton variant="quiet" size="md" @click="howDetection">How detection works</UiButton>
    </template>
  </EmptyState>

  <EmptyState v-else-if="shown.length === 0" title="No project matches" icon="search">
    Nothing matches "{{ query }}".
  </EmptyState>

  <ul v-else class="pl" aria-label="Projects">
    <li v-for="p in shown" :key="p.id">
      <button
        type="button"
        class="pl-card"
        :class="{ 'pl-card--sel': selectedId === p.id }"
        :aria-current="selectedId === p.id ? 'true' : undefined"
        @click="emit('select', p.id)"
      >
        <StatusDot class="pl-dot" :state="stateOf(p.id)" />
        <span class="pl-main">
          <span class="pl-name">{{ p.name }}</span>
          <span class="pl-path">{{ p.path || 'Folder not chosen yet' }}</span>
        </span>
        <UiChip v-if="p.kind">{{ p.kind }}</UiChip>
        <span class="pl-port">{{ (p.port ?? 0) > 0 ? `:${p.port}` : '' }}<template v-if="envOf(p.id)"> · {{ envOf(p.id) }}</template></span>
        <span class="pl-state" :class="`pl-state--${stateOf(p.id)}`">{{ actions.stateLabel(p) }}</span>
      </button>
    </li>
  </ul>
</template>

<style scoped>
.pl-dot { flex: none; }
.pl-state { font-size: var(--text-xs); color: var(--muted); white-space: nowrap; }
.pl-state--running { color: var(--green); }
.pl-state--starting, .pl-state--unresponsive { color: var(--amber); }
.pl-state--exited { color: var(--red); }

.pl { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-2); }
.pl-card {
  width: 100%;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto minmax(64px, max-content) 96px;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
  transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
}
.pl-card:hover { border-color: var(--border-strong); }
.pl-card:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.pl-card--sel { border-color: color-mix(in srgb, var(--accent) 55%, transparent); background: var(--surface2); }
.pl-main { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.pl-name { font-size: var(--text-sm); font-weight: 600; }
.pl-path { font: 400 var(--text-xs) var(--font-mono); color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.pl-port { font: 400 var(--text-xs) var(--font-mono); color: var(--muted); text-align: right; }
.pl-state { justify-self: end; }
@media (max-width: 760px) {
  .pl-card { grid-template-columns: auto minmax(0, 1fr) auto; }
  .pl-card :deep(.ui-chip), .pl-port { display: none; }
}
</style>
