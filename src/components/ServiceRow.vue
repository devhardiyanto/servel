<script setup lang="ts">
import ServiceLogo from '@/components/ui/ServiceLogo.vue'
import { statsAt } from '@/composables/useServices'
import { computed } from 'vue'
import type { ServiceDef, ServiceUiState } from '@/types/service'
import UiButton from '@/components/ui/UiButton.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import StatusDot, { type DotState } from '@/components/ui/StatusDot.vue'

const props = defineProps<{
  def: ServiceDef
  state: ServiceUiState
  connectOpen?: boolean
  /** Engine tidak menjawab: status sebenarnya tidak diketahui. */
  unknown?: boolean
}>()

const ramTitle = computed(() => {
  const ui = props.state
  if (ui?.status === 'running' && ui.memMb != null) {
    const t = statsAt.value ? new Date(statsAt.value).toTimeString().slice(0, 8) : ''
    return `Memory used by the container${t ? `, measured at ${t}` : ''}. Updates every 2 s on this screen.`
  }
  return `Estimate. Shown until ${props.def.name} runs and reports its real memory use.`
})

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
  /** Start ulang service yang dipilih tetapi tidak berjalan. */
  start: [id: string]
}>()

// Toggle = "dipilih". Dipilih tetapi tidak berjalan (auto-start
// dilewati, service berhenti sendiri) atau gagal → tampilkan keadaannya dan
// tombol untuk menyalakan lagi, supaya toggle on + stopped tidak tampak seperti
// keadaan yang benar.
const idle = computed(() => {
  const s = props.state.status
  return s === 'stopped' || s === 'not_created' || s === 'error'
})
const failed = computed(() => !props.unknown && idle.value && !!props.state.error)
const selectedIdle = computed(() => !props.unknown && props.state.selected && idle.value && !failed.value)

function formatPorts(def: ServiceDef): string {
  return def.ports.map((p) => `:${p.host}`).join(' / ')
}

function imageTag(def: ServiceDef): string {
  const parts = def.image.split(':')
  return parts.length > 1 ? `${parts[0].split('/').pop()}:${parts[1]}` : def.image
}

// Bentuk titik per status (lihat StatusDot): stopping → starting, error → exited.
const dot = computed<{ state: DotState; label: string }>(() => {
  if (props.unknown) return { state: 'stopped', label: 'unknown' }
  switch (props.state.status) {
    case 'running': return { state: 'running', label: 'running' }
    case 'starting': return { state: 'starting', label: 'starting' }
    case 'stopping': return { state: 'starting', label: 'stopping' }
    case 'error': return { state: 'exited', label: 'error' }
    default: return { state: 'stopped', label: 'stopped' }
  }
})
const statusLabel = computed(() => (failed.value ? 'failed' : selectedIdle.value ? 'not running' : dot.value.label))
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
    <div class="svc-icon"><ServiceLogo :id="def.id" /></div>
    <div class="svc-id">
      <span class="svc-name">{{ def.name }}</span>
      <span class="svc-version">{{ imageTag(def) }}</span>
    </div>
    <span class="svc-port">{{ formatPorts(def) }}</span>
    <span class="svc-ram" :title="ramTitle">{{ ramDisplay }}</span>
    <div class="svc-status-cell">
      <StatusDot :state="failed ? 'exited' : dot.state" :label="statusLabel" :title="failed ? state.error : undefined" />
      <button
        v-if="failed || selectedIdle"
        type="button"
        class="svc-act"
        :title="failed ? `${def.name} failed: ${state.error}` : `Start ${def.name}`"
        @click.stop="emit('start', def.id)"
      >{{ failed ? 'Retry' : 'Start' }}<span class="sr-only"> {{ def.name }}</span></button>
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
.svc-act {
  padding: 0;
  border: 0;
  background: none;
  font: inherit;
  font-size: var(--text-xs);
  color: var(--accent);
  cursor: pointer;
}
.svc-act:hover { text-decoration: underline; }
.svc-act:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 2px; }
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
  display: flex;
  align-items: center;
  justify-content: center;
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
  font-size: var(--text-md);
  font-weight: 600;
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
  /* RAM hanya keterangan: abu, bukan warna peringatan. */
  color: var(--muted);
  text-align: right;
  font-size: var(--text-xs);
}

.svc-status-cell {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  justify-content: center;
  gap: 1px;
  min-width: 0;
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
