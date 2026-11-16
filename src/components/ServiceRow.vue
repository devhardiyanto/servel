<script setup lang="ts">
import { computed } from 'vue'
import type { ServiceDef, ServiceUiState } from '@/types/service'
import UiButton from '@/components/ui/UiButton.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import StatusDot, { type DotState } from '@/components/ui/StatusDot.vue'

const props = defineProps<{
  def: ServiceDef
  state: ServiceUiState
  connectOpen?: boolean
}>()

const ramDisplay = computed(() => {
  const ui = props.state
  if (ui?.status === 'running' && ui.memMb != null) {
    return `${Math.round(ui.memMb)} MB`
  }
  return `~${props.def.ramEstimateMb} MB`
})

const emit = defineEmits<{
  toggle: [id: string]
  connect: [id: string]
}>()

const SERVICE_ICONS: Record<string, string> = {
  mysql: '🐬',
  postgres: '🐘',
  redis: '⚡',
  rabbitmq: '🐇',
  mongodb: '🍃',
  minio: '🗄️',
  mailpit: '✉️',
  gotenberg: '📄',
}

function icon(id: string): string {
  return SERVICE_ICONS[id] ?? id.slice(0, 2).toUpperCase()
}

function formatPorts(def: ServiceDef): string {
  return def.ports.map((p) => `:${p.host}`).join(' / ')
}

function imageTag(def: ServiceDef): string {
  const parts = def.image.split(':')
  return parts.length > 1 ? `${parts[0].split('/').pop()}:${parts[1]}` : def.image
}

// Bentuk titik per status (lihat StatusDot): stopping → starting, error → exited.
const dot = computed<{ state: DotState; label: string }>(() => {
  switch (props.state.status) {
    case 'running': return { state: 'running', label: 'running' }
    case 'starting': return { state: 'starting', label: 'starting' }
    case 'stopping': return { state: 'starting', label: 'stopping' }
    case 'error': return { state: 'exited', label: 'error' }
    default: return { state: 'stopped', label: 'stopped' }
  }
})
</script>

<template>
  <div
    class="svc-row"
    :class="{ 'svc-row--enabled': state.selected }"
    @click="emit('toggle', def.id)"
  >
    <UiToggle
      size="sm"
      :on="state.selected"
      :label="`Include ${def.name} in the selection`"
      @click.stop="emit('toggle', def.id)"
    />
    <div class="svc-icon">{{ icon(def.id) }}</div>
    <div class="svc-id">
      <span class="svc-name">{{ def.name }}</span>
      <span class="svc-version">{{ imageTag(def) }}</span>
    </div>
    <span class="svc-port">{{ formatPorts(def) }}</span>
    <span class="svc-ram">{{ ramDisplay }}</span>
    <div class="svc-status-cell">
      <StatusDot :state="dot.state" :label="dot.label" />
    </div>
    <UiButton
      variant="quiet"
      class="svc-connect"
      :aria-expanded="connectOpen"
      :title="`Connection details for ${def.name}`"
      @click.stop="emit('connect', def.id)"
    >Connect</UiButton>
  </div>
</template>

<style scoped>
.svc-row {
  display: grid;
  grid-template-columns: 36px 24px 1fr 100px 68px 84px 76px;
  align-items: center;
  gap: var(--space-3);
  padding: 7px var(--space-4);
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  cursor: pointer;
  transition: background 0.18s var(--ease);
}

.svc-row:hover {
  background: color-mix(in srgb, var(--accent) 5%, transparent);
}

.svc-row--enabled {
  background: color-mix(in srgb, var(--accent) 5%, transparent);
}

.svc-icon {
  font-size: var(--text-md);
  text-align: center;
  user-select: none;
}

.svc-id {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.svc-name {
  color: var(--text);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.svc-version {
  font-family: var(--font-mono);
  color: var(--faint);
  font-size: var(--text-xs);
}

.svc-port {
  font-family: var(--font-mono);
  color: var(--muted);
  font-size: var(--text-xs);
}

.svc-ram {
  font-family: var(--font-mono);
  color: var(--amber);
  text-align: right;
  font-size: var(--text-xs);
}

.svc-status-cell {
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.svc-connect {
  justify-self: end;
  padding: 2px 10px;
}
/* Tabel sempit (mis. panel Connect terbuka di jendela kecil): port tampil di
   panel Connect, jadi kolom port disembunyikan agar nama tidak terpotong. */
@container (max-width: 620px) {
  .svc-row { grid-template-columns: 36px 24px minmax(0, 1fr) 68px 84px 76px; }
  .svc-port { display: none; }
}
</style>
