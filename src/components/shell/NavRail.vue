<script setup lang="ts">
import { computed } from 'vue'
import KovaMark from './KovaMark.vue'
import type { ViewName } from '@/types/navigation'
import { useServices } from '@/composables/useServices'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime, isActive } from '@/composables/useProjectRuntime'
import { useLogs } from '@/composables/useLogs'
import { useUpdateCheck } from '@/composables/useUpdateCheck'

defineProps<{ current: ViewName }>()
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
const errorCount = computed(() => lines.value.filter((l) => l.level === 'error').length)

const items = computed(() => [
  { id: 'projects' as const, label: 'Projects', badge: runningProjects.value ? String(runningProjects.value) : '' },
  {
    id: 'services' as const,
    label: 'Services',
    badge: definitions.value.length ? `${runningCount.value}/${definitions.value.length}` : '',
  },
  { id: 'runtimes' as const, label: 'Runtimes', badge: '' },
  { id: 'logs' as const, label: 'Logs', badge: errorCount.value ? String(errorCount.value) : '', alert: errorCount.value > 0 },
  { id: 'settings' as const, label: 'Settings', badge: '', dot: updateAvailable.value },
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
      <span class="rail-label">{{ it.label }}</span>
      <span v-if="it.dot" class="rail-dot" title="Update available"></span>
      <span v-if="it.badge" class="rail-badge" :class="{ 'rail-badge--alert': it.alert }">{{ it.badge }}</span>
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
  padding: 7px var(--space-3);
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  text-align: left;
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.rail-item:hover { background: var(--surface); color: var(--text); }
.rail-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.rail-item--on { background: var(--surface2); color: var(--text); }
.rail-label { flex: 1; }
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
