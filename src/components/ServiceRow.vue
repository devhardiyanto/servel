<script setup lang="ts">
import type { ServiceDef, ServiceUiState } from '@/types/service'

defineProps<{
  def: ServiceDef
  state: ServiceUiState
}>()

const emit = defineEmits<{
  toggle: [id: string]
  stop: [id: string]
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
  sqlserver: '🏢',
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
    :class="{ 'svc-row--selected': state.selected }"
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
    <span class="svc-ram">~{{ def.ramEstimateMb }} MB</span>
    <div class="svc-status-cell">
      <button
        v-if="state.status === 'running' || state.status === 'stopping'"
        class="svc-stop-btn"
        :disabled="state.status === 'stopping'"
        @click.stop="emit('stop', def.id)"
      >
        ×
      </button>
      <span
        class="status-badge"
        :class="{
          'badge-run': state.status === 'running',
          'badge-stop': state.status === 'stopped' || state.status === 'not_created',
          'badge-starting': state.status === 'starting' || state.status === 'stopping',
          'badge-error': state.status === 'error',
        }"
      >
        <span class="sdot"></span>
        <span v-if="state.status === 'running'">up</span>
        <span v-else-if="state.status === 'starting'">···</span>
        <span v-else-if="state.status === 'stopping'">···</span>
        <span v-else-if="state.status === 'error'">err</span>
        <span v-else>off</span>
      </span>
    </div>
  </div>
</template>

<style scoped>
.svc-row {
  display: grid;
  grid-template-columns: 36px 24px 1fr 100px 68px 80px;
  align-items: center;
  gap: var(--space-3);
  padding: 5px var(--space-4);
  font-family: var(--font-mono);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.1s;
}

.svc-row:hover {
  background: color-mix(in srgb, var(--accent) 5%, transparent);
}

.svc-row--selected {
  background: color-mix(in srgb, var(--accent) 6%, transparent);
}

.svc-icon {
  font-size: 14px;
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
  color: var(--dim);
  font-size: 10px;
}

.svc-port {
  color: var(--muted);
  font-size: 11px;
}

.svc-ram {
  color: var(--amber);
  text-align: right;
  font-size: 11px;
}

.svc-toggle {
  width: 28px;
  height: 15px;
  border-radius: 8px;
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
  gap: var(--space-2);
}

.svc-stop-btn {
  font-family: var(--font-mono);
  font-size: 11px;
  width: 18px;
  height: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: var(--red);
  border: 1px solid color-mix(in srgb, var(--red) 40%, transparent);
  border-radius: 3px;
  cursor: pointer;
  transition: background 0.1s, border-color 0.1s;
  line-height: 1;
  padding: 0;
  flex-shrink: 0;
}

.svc-stop-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border-color: color-mix(in srgb, var(--red) 70%, transparent);
}

.svc-stop-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  letter-spacing: 0.04em;
}

.sdot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
}

.badge-run {
  color: var(--green);
}
.badge-run .sdot {
  background: var(--green);
  animation: pulse 2s infinite;
}

.badge-stop {
  color: var(--dim);
}
.badge-stop .sdot {
  background: var(--dim);
  animation: pulse 2.4s infinite;
}

.badge-starting {
  color: var(--amber);
}
.badge-starting .sdot {
  background: var(--amber);
  animation: spin-dot 1s linear infinite;
}

.badge-error {
  color: var(--red);
}
.badge-error .sdot {
  background: var(--red);
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

@keyframes spin-dot {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.2; }
}
</style>
