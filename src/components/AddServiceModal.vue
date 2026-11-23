<script setup lang="ts">
// Services › Add service / Edit service: service di luar katalog (contoh
// Elasticsearch). Dijalankan di engine dan network yang sama dengan service
// bawaan; backend merapikan definisinya (`sanitize_custom`).
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiModal from '@/components/ui/UiModal.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import { useConfig } from '@/composables/useConfig'
import { useServices } from '@/composables/useServices'
import { parseDotenv } from '@/utils/dotenv'
import type { ServiceDef } from '@/types/service'

const props = defineProps<{ open: boolean; edit?: ServiceDef | null }>()
const emit = defineEmits<{ close: []; saved: [id: string] }>()

const { saveCustomService } = useConfig()
const services = useServices()

const name = ref('')
const image = ref('')
const hostPort = ref('')
const innerPort = ref('')
const envText = ref('')
const dataPath = ref('')
const readyKind = ref<'port' | 'http'>('port')
const readyPath = ref('/')
const connectEnv = ref('')
const error = ref('')
const saving = ref(false)
const portCheck = ref<{ free: boolean; reason: string } | null>(null)

/** Id dari nama: huruf kecil, angka, `-` (sama dengan `valid_custom_id`). */
function slug(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 32)
}
const id = computed(() => props.edit?.id ?? slug(name.value))
const taken = computed(() => !props.edit && services.definitions.value.some((d) => d.id === id.value))

watch(() => props.open, (o) => {
  if (!o) return
  const d = props.edit
  error.value = ''
  name.value = d?.name ?? ''
  image.value = d?.image ?? ''
  hostPort.value = d?.ports[0]?.host ?? ''
  innerPort.value = d?.ports[0]?.container ?? ''
  envText.value = Object.entries(d?.environment ?? {}).map(([k, v]) => `${k}=${v}`).join('\n')
  dataPath.value = d?.volumes?.[0]?.split(':').slice(1).join(':') ?? ''
  readyKind.value = d?.ready?.kind === 'http' ? 'http' : 'port'
  readyPath.value = d?.ready?.kind === 'http' ? d.ready.path : '/'
  connectEnv.value = d?.connectEnv ?? ''
})

// Port di PC: isi otomatis sama dengan port container, lalu cek bebas.
watch(innerPort, (v, old) => {
  if (!props.edit && (hostPort.value === '' || hostPort.value === old)) hostPort.value = v
})
let timer: ReturnType<typeof setTimeout> | undefined
watch(hostPort, (raw) => {
  clearTimeout(timer)
  portCheck.value = null
  const n = Number(raw)
  if (!raw || !Number.isInteger(n)) return
  if (props.edit && raw === props.edit.ports[0]?.host) return
  timer = setTimeout(async () => {
    try {
      portCheck.value = await invoke<{ free: boolean; reason: string }>('project_port_check', { port: n, taken: [] })
    } catch {
      portCheck.value = null
    }
  }, 250)
})

const problem = computed(() => {
  if (!name.value.trim()) return 'Give the service a name.'
  if (!id.value) return 'Use letters or digits in the name.'
  if (taken.value) return `A service called ${id.value} already exists.`
  if (!image.value.trim() || /\s/.test(image.value.trim())) return 'Enter an image, e.g. elasticsearch:8.15.0.'
  const h = Number(hostPort.value)
  const c = Number(innerPort.value)
  if (!Number.isInteger(c) || c < 1 || c > 65535) return 'Enter the port the service listens on inside the container.'
  if (!Number.isInteger(h) || h < 1024 || h > 65535) return 'Port on this PC must be 1024–65535.'
  if (dataPath.value && !dataPath.value.startsWith('/')) return 'The data folder is a path inside the container, starting with /.'
  return ''
})

function build(): ServiceDef {
  const vid = id.value.replace(/-/g, '_')
  const env = Object.fromEntries(parseDotenv(envText.value).entries.map((e) => [e.k, e.v]))
  return {
    id: id.value,
    name: name.value.trim(),
    category: 'custom',
    image: image.value.trim(),
    containerName: `servel_${vid}`,
    ports: [{ host: String(Number(hostPort.value)), container: String(Number(innerPort.value)) }],
    environment: env,
    volumes: dataPath.value ? [`servel_${vid}_data:${dataPath.value}`] : undefined,
    ramEstimateMb: props.edit?.ramEstimateMb ?? 200,
    custom: true,
    ready: readyKind.value === 'http' ? { kind: 'http', path: readyPath.value || '/' } : { kind: 'port' },
    connectEnv: connectEnv.value.trim() || null,
  }
}

async function save(start: boolean): Promise<void> {
  if (problem.value) return
  saving.value = true
  error.value = ''
  try {
    const def = build()
    await saveCustomService(def)
    await services.reloadDefinitions()
    if (start) {
      if (!services.uiState.value[def.id]?.selected) services.toggle(def.id)
      if (props.edit && services.uiState.value[def.id]?.status === 'running') await services.restart(def.id)
      else await services.start([def.id])
    }
    emit('saved', def.id)
    emit('close')
  } catch (err) {
    error.value = String(err)
  } finally {
    saving.value = false
  }
}

const readyOptions = [
  { value: 'port', label: 'Port answers' },
  { value: 'http', label: 'HTTP 200 on a path' },
]
</script>

<template>
  <UiModal :open="open" :title="edit ? `Edit ${edit.name}` : 'Add service'" @close="emit('close')">
    <p class="asm-text">Runs on the same engine and network as the built-in services. Other containers reach it by its name.</p>
    <div class="asm-grid">
      <UiInput v-model="name" label="Name" placeholder="Elasticsearch" :disabled="!!edit" data-autofocus />
      <UiInput v-model="image" label="Image" mono placeholder="elasticsearch:8.15.0" help="From Docker Hub or a registry this PC can reach" />
      <UiInput v-model="innerPort" label="Port in the container" mono placeholder="9200" />
      <UiInput
        v-model="hostPort"
        label="Port on this PC"
        mono
        placeholder="9200"
        :error="portCheck && !portCheck.free ? portCheck.reason : undefined"
        :help="portCheck?.free ? `${hostPort} is free` : undefined"
      />
    </div>
    <UiInput v-model="envText" as="textarea" :rows="3" mono label="Environment" placeholder="discovery.type=single-node&#10;xpack.security.enabled=false" help="One KEY=VALUE per line" />
    <UiInput v-model="dataPath" label="Data folder in the container" mono placeholder="/usr/share/elasticsearch/data" :help="dataPath ? `Kept in the volume servel_${id.replace(/-/g, '_')}_data when the service stops` : 'Leave empty if the service keeps no data'" />
    <div class="asm-ready">
      <span class="asm-label">Ready when</span>
      <UiSegmented v-model="readyKind" :options="readyOptions" label="Ready when" />
      <UiInput v-if="readyKind === 'http'" v-model="readyPath" mono aria-label="Path to check" placeholder="/_cluster/health" />
    </div>
    <UiInput v-model="connectEnv" as="textarea" :rows="2" mono label="Connect values" placeholder="ELASTICSEARCH_URL=http://localhost:{port}" help="Shown in Connect and copied with Copy .env. {port} is the port on this PC." />
    <p v-if="problem && (name || image)" class="asm-err">{{ problem }}</p>
    <p v-if="error" class="asm-err" role="alert">{{ error }}</p>
    <template #footer>
      <UiButton size="md" @click="emit('close')">Cancel</UiButton>
      <UiButton size="md" :disabled="!!problem" :loading="saving" @click="save(false)">{{ edit ? 'Save' : 'Add' }}</UiButton>
      <UiButton variant="primary" size="md" :disabled="!!problem" :loading="saving" @click="save(true)">{{ edit ? 'Save and restart' : 'Add and start' }}</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.asm-text { margin: 0; color: var(--muted); font-size: var(--text-sm); }
.asm-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-3); }
.asm-ready { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); }
.asm-label { font-size: var(--text-xs); font-weight: 500; color: var(--muted); width: 100%; }
.asm-err { margin: 0; color: var(--red); font-size: var(--text-sm); }
</style>
