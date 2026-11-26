<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import KovaMark from './KovaMark.vue'
import type { ViewName } from '@/types/navigation'
import { useServices } from '@/composables/useServices'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime, isActive } from '@/composables/useProjectRuntime'
import { useLogs } from '@/composables/useLogs'
import { useUpdateCheck } from '@/composables/useUpdateCheck'

const props = defineProps<{ current: ViewName }>()
const emit = defineEmits<{ go: [view: ViewName]; palette: [] }>()

const isMac = /mac/i.test(navigator.platform)

const { definitions, runningCount } = useServices()
const { config } = useConfig()
const { stateOf } = useProjectRuntime()
const { lines } = useLogs()
const { updateAvailable } = useUpdateCheck()

const runningProjects = computed(
  () => config.value.projects.filter((p) => isActive(stateOf(p.id))).length,
)
// Errors the user has not dealt with yet: lines after the last visit to Logs,
// minus those of projects that are running again (they recovered).
const seenSeq = ref(0)
watch(
  () => [props.current, lines.value[lines.value.length - 1]?.seq ?? 0] as const,
  ([view, last]) => {
    if (view === 'logs') seenSeq.value = last
  },
  { immediate: true },
)
const recovered = computed(
  () => new Set(config.value.projects.filter((p) => stateOf(p.id) === 'running').map((p) => p.name)),
)
// Dibanding nomor urut, bukan posisi: baris yang sudah dilihat bisa dibuang buffer.
const errorCount = computed(() => {
  let n = 0
  for (const l of lines.value) {
    if ((l.seq ?? 0) > seenSeq.value && l.level === 'error' && !recovered.value.has(l.src)) n++
  }
  return n
})

// Ikon stroke 16px untuk item nav rail.
const ICONS: Record<ViewName, string[]> = {
  projects: ['M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z'],
  services: ['M3 4a5 2 0 1 0 10 0a5 2 0 1 0 -10 0', 'M3 4v8c0 1.1 2.2 2 5 2s5-.9 5-2V4M3 8c0 1.1 2.2 2 5 2s5-.9 5-2'],
  runtimes: ['M8 2l6 3-6 3-6-3zM2 8l6 3 6-3M2 11l6 3 6-3'],
  logs: ['M3 3.5h10M3 6.5h10M3 9.5h7M3 12.5h5'],
  settings: ['M2.5 4h7M12.5 4h1M2.5 8h2M7.5 8h6M2.5 12h8', 'M9.5 4a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0M4.5 8a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0M10.5 12a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0'],
  onboarding: [],
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

// `badgeLabel` dibacakan pembaca layar; angkanya sendiri tidak bermakna tanpa konteks.
const items = computed(() => [
  {
    id: 'projects' as const,
    label: 'Projects',
    badge: runningProjects.value ? String(runningProjects.value) : '',
    badgeLabel: `${plural(runningProjects.value, 'project', 'projects')} running`,
  },
  {
    id: 'services' as const,
    label: 'Services',
    badge: definitions.value.length ? `${runningCount.value}/${definitions.value.length}` : '',
    badgeLabel: `${runningCount.value} of ${plural(definitions.value.length, 'service', 'services')} running`,
  },
  { id: 'runtimes' as const, label: 'Runtimes', badge: '', badgeLabel: '' },
  {
    id: 'logs' as const,
    label: 'Logs',
    badge: errorCount.value ? String(errorCount.value) : '',
    badgeLabel: plural(errorCount.value, 'error', 'errors'),
    alert: errorCount.value > 0,
  },
  { id: 'settings' as const, label: 'Settings', badge: '', badgeLabel: '', dot: updateAvailable.value },
])
</script>

<template>
  <nav class="rail" aria-label="Main">
    <div class="rail-brand">
      <KovaMark :size="24" />
      <span class="rail-name">Kova</span>
    </div>
    <button
      v-for="it in items"
      :key="it.id"
      class="rail-item"
      :class="{ 'rail-item--on': current === it.id }"
      :aria-current="current === it.id ? 'page' : undefined"
      @click="emit('go', it.id)"
    >
      <svg class="rail-icon" viewBox="0 0 16 16" aria-hidden="true">
        <path v-for="(d, i) in ICONS[it.id]" :key="i" :d="d" />
      </svg>
      <span class="rail-label">{{ it.label }}</span>
      <span v-if="it.dot" class="rail-dot" role="img" aria-label="Update available" title="Update available"></span>
      <span v-if="it.badge" class="rail-badge" :class="{ 'rail-badge--alert': it.alert }">
        <span aria-hidden="true">{{ it.badge }}</span>
        <span class="sr-only">, {{ it.badgeLabel }}</span>
      </span>
    </button>
    <button class="rail-cmd" title="Open the command palette" @click="emit('palette')">
      <span class="rail-label">Commands</span>
      <kbd>{{ isMac ? '⌘K' : 'Ctrl K' }}</kbd>
    </button>
  </nav>
</template>

<style scoped>
.rail {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: var(--space-3) var(--space-2);
  background: var(--bg-sunken);
  border-right: 1px solid var(--border);
  min-height: 0;
}
.rail-brand {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2) var(--space-4);
}
.rail-name { font-size: var(--text-md); font-weight: 600; color: var(--text); letter-spacing: 0.01em; }
.rail-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  height: 34px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.rail-item:hover { background: var(--surface); color: var(--text); }
.rail-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.rail-item--on { background: var(--surface2); color: var(--text); }
.rail-label { flex: 1; }
.rail-icon {
  width: 16px;
  height: 16px;
  flex: none;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.rail-cmd {
  display: flex;
  align-items: center;
  margin-top: auto;
  padding: var(--space-2) var(--space-3);
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--faint);
  font-size: var(--text-xs);
  text-align: left;
  cursor: pointer;
}
.rail-cmd:hover { color: var(--text); border-color: var(--border-strong); }
.rail-cmd:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.rail-cmd kbd { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--faint); }
.rail-badge {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--faint);
}
.rail-badge--alert { color: var(--red); }
.rail-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); }
</style>
