<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { useEngineStatus } from '@/composables/useEngineStatus'
import { useServices } from '@/composables/useServices'

const { engineStatus, engineLabel } = useEngineStatus()
const { definitions, uiState, runningCount } = useServices()
const version = ref('')

const ramMb = computed(() =>
  Object.values(uiState.value)
    .filter((s) => s.status === 'running' && s.memMb != null)
    .reduce((sum, s) => sum + (s.memMb ?? 0), 0),
)
const engineText = computed(() =>
  engineStatus.value === 'up' ? 'running' : engineStatus.value === 'down' ? 'not responding' : 'checking',
)

onMounted(async () => {
  try {
    version.value = await getVersion()
  } catch {
    version.value = ''
  }
})
</script>

<template>
  <footer class="sb" role="status">
    <span class="sb-item">
      <span class="sb-dot" :class="`sb-dot--${engineStatus}`"></span>
      {{ engineLabel }} · {{ engineText }}
    </span>
    <span class="sb-item">{{ runningCount }}/{{ definitions.length }} services</span>
    <span v-if="ramMb > 0" class="sb-item">RAM {{ ramMb >= 1024 ? `${(ramMb / 1024).toFixed(1)} GB` : `${Math.round(ramMb)} MB` }}</span>
    <span class="sb-spacer"></span>
    <span v-if="version" class="sb-item">v{{ version }}</span>
  </footer>
</template>

<style scoped>
.sb {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  height: 26px;
  padding: 0 var(--space-4);
  background: var(--bg-sunken);
  border-top: 1px solid var(--border);
  font-size: var(--text-xs);
  color: var(--faint);
  white-space: nowrap;
}
.sb-item { display: inline-flex; align-items: center; gap: 6px; }
.sb-spacer { flex: 1; }
.sb-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); }
.sb-dot--up { background: var(--green); }
.sb-dot--down { background: var(--red); }
</style>
