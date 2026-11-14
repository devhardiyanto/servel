<script setup lang="ts">
import { computed } from 'vue'
import type { ServiceDef, ServiceUiState } from '@/types/service'
import UiButton from '@/components/ui/UiButton.vue'

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
</script>

<template>
  <div
    class="svc-row"
    :class="{ 'svc-row--enabled': state.selected }"
    @click="emit('toggle', def.id)"
  >
    <div class="svc-toggle" :class="{ 'svc-toggle--on': state.selected }" @click.stop="emit('toggle', def.id)">
      <span class="svc-knob" :class="{ 'svc-knob--on': state.selected }"></span>
    </div>
    <div class="svc-icon">{{ icon(def.id) }}</div>
    <div class="svc-id">
      <span class="svc-name">{{ def.name }}</span>
      <span class="svc-version">{{ imageTag(def) }}</span>
    </div>
    <span class="svc-port">{{ formatPorts(def) }}</span>
    <span class="svc-ram">{{ ramDisplay }}</span>
    <div class="svc-status-cell">
      <span
        v-if="state.status === 'starting' || state.status === 'stopping'"
        class="status-badge badge-starting"
      >
        <span class="sdot"></span>&#8943;
      </span>
      <span
        v-else-if="state.status === 'running'"
        class="status-badge badge-run"
      >
        <span class="sdot"></span>up
      </span>
      <span
        v-else-if="state.status === 'error'"
        class="status-badge badge-error"
      >
        <span class="sdot"></span>err
      </span>
      <span
        v-else
        class="status-badge badge-stop"
      >
        <span class="sdot"></span>off
      </span>
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
  grid-template-columns: 36px 24px 1fr 100px 68px 80px 76px;
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

.svc-toggle {
  width: 28px;
  height: 15px;
  border-radius: var(--radius-md);
  background: var(--surface2);
  border: 1px solid var(--border);
  position: relative;
  flex-shrink: 0;
  transition: background 0.15s, border-color 0.15s;
}

.svc-toggle--on {
  background: color-mix(in srgb, var(--accent) 30%, transparent);
  border-color: color-mix(in srgb, var(--accent) 60%, transparent);
}

.svc-knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--muted);
  transition: left 0.15s, background 0.15s;
}

.svc-knob--on {
  left: 15px;
  background: var(--accent);
}

.svc-status-cell {
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: var(--text-xs);
  letter-spacing: 0.01em;
}

.sdot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  position: relative;
}

/* Running — green, soft "alive" pulse with halo */
.badge-run {
  color: var(--green);
}
.badge-run .sdot {
  background: var(--green);
  box-shadow: 0 0 0 0 color-mix(in srgb, var(--green) 60%, transparent);
  animation: heartbeat-run 1.8s ease-in-out infinite;
}

/* Stopped — dim, subtle slow pulse (idle) */
.badge-stop {
  color: var(--faint);
}
.badge-stop .sdot {
  background: var(--dim);
  animation: heartbeat-stop 3s ease-in-out infinite;
}

/* Starting / stopping — amber, fast urgent pulse */
.badge-starting {
  color: var(--amber);
}
.badge-starting .sdot {
  background: var(--amber);
  animation: heartbeat-active 0.9s ease-in-out infinite;
}

/* Error — red, static */
.badge-error {
  color: var(--red);
}
.badge-error .sdot {
  background: var(--red);
}

@keyframes heartbeat-run {
  0%, 100% {
    transform: scale(1);
    box-shadow: 0 0 0 0 color-mix(in srgb, var(--green) 50%, transparent);
  }
  50% {
    transform: scale(1.15);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--green) 0%, transparent);
  }
}

@keyframes heartbeat-stop {
  0%, 100% { opacity: 0.55; }
  50% { opacity: 0.85; }
}

@keyframes heartbeat-active {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.4; transform: scale(0.8); }
}
.svc-connect {
  justify-self: end;
  padding: 2px 10px;
}
</style>
