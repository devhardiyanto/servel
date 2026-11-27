<script setup lang="ts">
import { computed, inject, onMounted, onUnmounted, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useEngineStatus } from '@/composables/useEngineStatus'
import { useServices } from '@/composables/useServices'
import { SetViewKey } from '@/types/navigation'
import { summarizeRam } from '@/utils/ram'

const { engineStatus, engineLabel } = useEngineStatus()
const { definitions, uiState, runningCount } = useServices()
const version = ref('')

/** Payload `ram-stats-changed` dari polling backend (tiap 2 detik selama jendela terlihat). */
interface RamEvent { servicesMb: number | null; projectsMb: number; kovaMb: number; measuredAt: number }
const proc = ref<RamEvent | null>(null)

// Angka service dari state yang sama dengan layar Services: service berjalan
// tanpa angka terukur memakai perkiraannya.
const services = computed(() => {
  let mb = 0
  let estimated = false
  for (const d of definitions.value) {
    const s = uiState.value[d.id]
    if (s?.status !== 'running') continue
    if (s.memMb != null) mb += s.memMb
    else {
      mb += d.ramEstimateMb
      estimated = true
    }
  }
  return { mb, estimated }
})
const ram = computed(() =>
  summarizeRam({
    servicesMb: services.value.mb,
    servicesEstimated: services.value.estimated,
    projectsMb: proc.value?.projectsMb ?? null,
    kovaMb: proc.value?.kovaMb ?? null,
    measuredAt: proc.value?.measuredAt ?? null,
  }),
)

const ENGINE_TEXT = { up: 'running', down: 'not responding', missing: 'not installed', unknown: 'checking' } as const
const engineText = computed(() => ENGINE_TEXT[engineStatus.value])
// Engine bermasalah: klik membuka Settings (Container engine ada di General).
const setView = inject(SetViewKey, () => {})
const engineActionable = computed(() => engineStatus.value === 'down' || engineStatus.value === 'missing')

let unlisten: UnlistenFn | null = null
onMounted(async () => {
  try {
    version.value = await getVersion()
  } catch {
    version.value = ''
  }
  try {
    unlisten = await listen<RamEvent>('ram-stats-changed', (e) => (proc.value = e.payload))
  } catch {
    /* tanpa backend (mis. pratinjau): RAM proses tidak ditampilkan */
  }
})
onUnmounted(() => unlisten?.())
</script>

<template>
  <!-- Hanya status engine yang diumumkan. RAM berganti tiap 2 detik, jadi
       sengaja di luar live region supaya pembaca layar tidak terus berbicara. -->
  <footer class="sb">
    <span role="status" class="sb-live">
      <button
        v-if="engineActionable"
        type="button"
        class="sb-item sb-link"
        title="Open container engine settings"
        @click="setView('settings')"
      >
        <span class="sb-dot" :class="`sb-dot--${engineStatus}`"></span>
        {{ engineLabel }} · {{ engineText }}
      </button>
      <span v-else class="sb-item">
        <span class="sb-dot" :class="`sb-dot--${engineStatus}`"></span>
        {{ engineLabel }} · {{ engineText }}
      </span>
    </span>
    <span class="sb-item">{{ engineActionable ? '–' : runningCount }}/{{ definitions.length }} services</span>
    <span v-if="ram && !engineActionable" class="sb-item sb-ram" aria-live="off" :title="ram.title">{{ ram.text }}</span>
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
.sb-live { display: inline-flex; }
.sb-item { display: inline-flex; align-items: center; gap: 6px; }
/* Angka berganti tiap 2 detik: lebar digit tetap supaya isi bar tidak bergeser. */
.sb-ram { font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
.sb-spacer { flex: 1; }
.sb-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); }
.sb-dot--up { background: var(--green); }
.sb-dot--down,
.sb-dot--missing { background: var(--red); }
.sb-link { padding: 0; border: 0; background: none; font: inherit; color: inherit; cursor: pointer; }
.sb-link:hover { color: var(--text); text-decoration: underline; }
.sb-link:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 2px; }
</style>
