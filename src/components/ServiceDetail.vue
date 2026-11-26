<script setup lang="ts">
// Panel detail service: status dan aksi,
// lalu tab Connect (nilai koneksi, .env dalam tiga format), Data (volume dan
// restore), Logs (keluaran operasi Kova untuk service ini).
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import UiButton from '@/components/ui/UiButton.vue'
import UiModal from '@/components/ui/UiModal.vue'
import ServiceLogo from '@/components/ui/ServiceLogo.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import { tabPanelAttrs } from '@/utils/tabs'
import UiTabs from '@/components/ui/UiTabs.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import StatusDot, { type DotState } from '@/components/ui/StatusDot.vue'
import LogTail from '@/components/LogTail.vue'
import PortOwnerBanner from '@/components/PortOwnerBanner.vue'
import ServicePortModal from '@/components/ServicePortModal.vue'
import { lastPortChange } from '@/utils/portChange'
import { useServices } from '@/composables/useServices'
import { useLogs } from '@/composables/useLogs'
import { useToast } from '@/composables/useToast'
import { connectionInfo, envFor, type EnvFormat } from '@/utils/connect'
import type { ServiceDef } from '@/types/service'

const props = defineProps<{ def: ServiceDef; focusTab?: { tab: 'connect' | 'data' | 'logs'; n: number } | null }>()
const emit = defineEmits<{ close: [] }>()

const services = useServices()
const { lines } = useLogs()
const toast = useToast()

const tab = ref<'connect' | 'data' | 'logs'>('connect')
const format = ref<EnvFormat>('laravel')
// Menu ⋯ › View logs membuka panel ini di tab Logs.
watch(() => props.focusTab?.n, () => {
  if (props.focusTab) tab.value = props.focusTab.tab
}, { immediate: true })

const ui = computed(() => services.uiState.value[props.def.id])
const status = computed(() => ui.value?.status ?? 'stopped')
const running = computed(() => status.value === 'running')
const busy = computed(() => status.value === 'starting' || status.value === 'stopping')
const dot = computed<DotState>(() => {
  switch (status.value) {
    case 'running': return 'running'
    case 'starting':
    case 'stopping': return 'starting'
    case 'error': return 'exited'
    default: return 'stopped'
  }
})
const statusText = computed(() => (status.value === 'not_created' ? 'not created' : status.value))
const conflict = computed(() => services.portConflicts.value.find((c) => c.serviceId === props.def.id) ?? null)
const portOpen = ref(false)
/** Port bebas berikutnya untuk tombol Change port… di banner konflik. */
const conflictSuggestion = ref<number | null>(null)
watch(conflict, async (c) => {
  conflictSuggestion.value = null
  if (!c) return
  try {
    const r = await invoke<{ suggestion: number | null }>('project_port_check', { port: c.port, taken: [] })
    conflictSuggestion.value = r.suggestion ?? c.port + 1
  } catch {
    conflictSuggestion.value = c.port + 1
  }
}, { immediate: true })

const info = computed(() => connectionInfo(props.def))
const env = computed(() => envFor(props.def, format.value))

async function copy(text: string, what: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
    toast.show(what)
  } catch {
    toast.show('Could not copy. Select the text and copy it by hand.', { tone: 'info' })
  }
}

// Start/Stop di panel = saklar di baris: pilihan profil ikut berubah.
async function toggleRun(): Promise<void> {
  const selected = !!ui.value?.selected
  if (running.value) {
    if (selected) services.toggle(props.def.id)
    else await services.stop([props.def.id])
  } else if (!selected) services.toggle(props.def.id)
  else await services.start([props.def.id])
}
const restorable = computed(() => props.def.id === 'mysql' || props.def.id === 'postgres')
const volumes = computed(() => props.def.volumes ?? [])
const restoring = ref(false)
const restoreMsg = ref<{ ok: boolean; text: string } | null>(null)

/** Chosen dump, waiting for the confirmation dialog. */
const restoreFile = ref<string | null>(null)
async function restore(): Promise<void> {
  const file = await openDialog({ multiple: false, title: `Choose a .sql dump for ${props.def.name}`, filters: [{ name: 'SQL', extensions: ['sql'] }] })
  if (typeof file === 'string') restoreFile.value = file
}
async function confirmRestore(): Promise<void> {
  const file = restoreFile.value
  restoreFile.value = null
  if (!file) return
  restoring.value = true
  restoreMsg.value = null
  try {
    await invoke('services_restore', { service: props.def.id, file })
    restoreMsg.value = { ok: true, text: 'Restore finished. Details are on the Logs tab.' }
  } catch (err) {
    restoreMsg.value = { ok: false, text: String(err) }
  } finally {
    restoring.value = false
  }
}

const serviceLines = computed(() => lines.value.filter((l) => l.src === props.def.id.toUpperCase()).slice(-300))

const tabs = [
  { value: 'connect', label: 'Connect' },
  { value: 'data', label: 'Data' },
  { value: 'logs', label: 'Logs' },
]
const formats = [
  { value: 'laravel', label: 'Laravel' },
  { value: 'url', label: 'URL' },
  { value: 'go', label: 'Go' },
]
function dismissPortChange(): void {
  lastPortChange.value = null
}
</script>

<template>
  <div class="sd">
    <header class="sd-head">
      <div class="sd-title">
        <h2 class="sd-name"><ServiceLogo :id="def.id" :size="20" />{{ def.name }}</h2>
        <UiButton variant="quiet" aria-label="Close service details" @click="emit('close')">&times;</UiButton>
      </div>
      <div class="sd-meta">
        <StatusDot :state="dot" :label="statusText" />
        <span class="sd-mono">{{ def.image }}</span>
        <span v-if="ui?.memMb != null" class="sd-mono">{{ Math.round(ui.memMb) }} MB</span>
      </div>
      <div class="sd-actions">
        <UiButton :variant="running ? 'default' : 'success'" :disabled="busy" @click="toggleRun">{{ running ? 'Stop' : 'Start' }}</UiButton>
        <UiButton v-if="info.webUrl" variant="quiet" @click="openUrl(info.webUrl!)">Open web UI</UiButton>
      </div>
    </header>

    <UiTabs v-model="tab" :tabs="tabs" label="Service details" id-base="sd" />

    <div v-if="tab === 'connect'" class="sd-body" v-bind="tabPanelAttrs('sd', tab)">
      <template v-if="conflict">
        <PortOwnerBanner
          :key="conflict.port"
          :port="conflict.port"
          :subject="def.name"
          :suggestion="conflictSuggestion"
          suggestion-label="Change port…"
          @use-port="portOpen = true"
          @stopped="services.startIgnoringConflicts()"
        />
        <div class="sd-btns">
          <UiButton variant="quiet" @click="services.startIgnoringConflicts()">Start anyway</UiButton>
          <UiButton variant="quiet" @click="services.dismissPortConflicts()">Close</UiButton>
        </div>
      </template>
      <p v-else-if="!running" class="sd-note">{{ def.name }} is not running. These values work once it starts.</p>
      <UiBanner
        v-if="lastPortChange?.serviceId === def.id && lastPortChange.projects.length"
        tone="warn"
        :title="`${lastPortChange.projects.length} ${lastPortChange.projects.length === 1 ? 'project uses' : 'projects use'} ${lastPortChange.from}`"
      >
        {{ lastPortChange.projects.join(', ') }} still {{ lastPortChange.projects.length === 1 ? 'has' : 'have' }} {{ lastPortChange.from }} in the Environment tab. Update {{ lastPortChange.projects.length === 1 ? 'it' : 'them' }} to {{ lastPortChange.to }}. A project's own .env file is not checked.
        <template #actions><UiButton variant="quiet" @click="dismissPortChange">Done</UiButton></template>
      </UiBanner>

      <dl class="sd-fields">
        <template v-for="f in info.fields" :key="f.label">
          <dt>{{ f.label }}</dt>
          <dd class="sd-mono">{{ f.value }}</dd>
        </template>
      </dl>

      <div v-if="info.env" class="sd-env">
        <div class="sd-env__bar">
          <span class="sd-label">.env for</span>
          <UiSegmented v-model="format" :options="formats" label="Format" />
        </div>
        <pre class="sd-env__code" tabindex="0" aria-label=".env lines">{{ env }}</pre>
      </div>

      <div class="sd-btns">
        <UiButton variant="quiet" @click="portOpen = true">Change port…</UiButton>
        <UiButton variant="quiet" @click="copy(info.url, `Copied the ${def.name} URL`)">Copy URL</UiButton>
        <UiButton v-if="env" variant="accent" @click="copy(env, `Copied .env for ${def.name} (${env.split('\n').length} line${env.split('\n').length === 1 ? '' : 's'})`)">Copy .env</UiButton>
      </div>
    </div>

    <div v-else-if="tab === 'data'" class="sd-body" v-bind="tabPanelAttrs('sd', tab)">
      <section class="sd-sec">
        <h3 class="sd-label">Volumes</h3>
        <ul v-if="volumes.length" class="sd-list">
          <li v-for="v in volumes" :key="v" class="sd-mono">{{ v }}</li>
        </ul>
        <p v-else class="sd-note">{{ def.name }} keeps no data between restarts.</p>
        <p class="sd-note">Volumes stay when the service stops or Kova updates. Switching engines (WSL containers ⇄ Docker) starts with empty volumes.</p>
      </section>
      <section v-if="restorable" class="sd-sec">
        <h3 class="sd-label">Restore database</h3>
        <p class="sd-note">Import a .sql dump. Kova starts {{ def.name }}, waits until it accepts connections, then runs the dump.</p>
        <div><UiButton :loading="restoring" @click="restore">Choose dump…</UiButton></div>
        <UiBanner v-if="restoreMsg" :tone="restoreMsg.ok ? 'ok' : 'err'" :title="restoreMsg.ok ? 'Restored' : 'Restore failed'">{{ restoreMsg.text }}</UiBanner>
      </section>
    </div>

    <div v-else class="sd-body" v-bind="tabPanelAttrs('sd', tab)">
      <LogTail v-if="serviceLines.length" :lines="serviceLines" max-height="calc(100vh - 320px)" />
      <p v-else class="sd-note">No output from Kova for {{ def.name }} in this session.</p>
      <p class="sd-note">Container output: <code class="sd-mono">kova logs {{ def.id }} -f</code></p>
    </div>

    <ServicePortModal :open="portOpen" :def="def" :suggestion="conflict ? conflictSuggestion : null" @close="portOpen = false" />
    <UiModal :open="!!restoreFile" :title="`Restore into ${def.name}?`" size="sm" danger @close="restoreFile = null">
      <p class="sd-confirm">Kova runs <code class="sd-mono">{{ restoreFile?.split(/[\\/]/).pop() }}</code> against {{ def.name }}. Most dumps drop and recreate the tables they contain, so existing data in those tables is replaced. This cannot be undone.</p>
      <template #footer>
        <UiButton size="md" data-autofocus @click="restoreFile = null">Cancel</UiButton>
        <UiButton variant="danger" size="md" @click="confirmRestore">Restore</UiButton>
      </template>
    </UiModal>
  </div>
</template>

<style scoped>
.sd { display: flex; flex-direction: column; gap: var(--space-3); min-width: 0; }
.sd-head { display: flex; flex-direction: column; gap: var(--space-2); }
.sd-title { display: flex; align-items: center; justify-content: space-between; }
.sd-name { margin: 0; font-size: var(--text-lg); font-weight: 600; color: var(--text); }
.sd-meta { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-3); font-size: var(--text-xs); color: var(--muted); }
.sd-actions, .sd-btns { display: flex; flex-wrap: wrap; gap: var(--space-2); }
.sd-btns { justify-content: flex-end; }
.sd-body { display: flex; flex-direction: column; gap: var(--space-4); font-size: var(--text-sm); }
.sd-mono { font-family: var(--font-mono); font-size: var(--text-xs); }
.sd-note { margin: 0; color: var(--faint); font-size: var(--text-xs); }
.sd-label { margin: 0; font-size: var(--text-xs); font-weight: 500; color: var(--muted); }
.sd-fields { display: grid; grid-template-columns: max-content 1fr; gap: var(--space-2) var(--space-4); margin: 0; }
.sd-fields dt { color: var(--faint); font-size: var(--text-xs); }
.sd-fields dd { margin: 0; color: var(--text); overflow-wrap: anywhere; }
.sd-env { display: flex; flex-direction: column; gap: var(--space-2); }
.sd-env__bar { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
.sd-env__code {
  margin: 0;
  padding: var(--space-3);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-sunken);
  font: 400 var(--text-xs) / 1.7 var(--font-mono);
  color: var(--text);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.sd-env__code:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.sd-sec { display: flex; flex-direction: column; gap: var(--space-2); }
.sd-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-1); }
.sd-confirm { margin: 0; line-height: 1.6; }
.sd-name { display: inline-flex; align-items: center; gap: var(--space-2); }
</style>
