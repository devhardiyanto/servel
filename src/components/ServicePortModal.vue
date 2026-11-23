<script setup lang="ts">
// Services › Change port: port di PC untuk satu service. Di dalam container
// port tidak berubah. Container dibuat ulang saat start berikutnya (definisi
// berubah); volume tetap.
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiModal from '@/components/ui/UiModal.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { useConfig } from '@/composables/useConfig'
import { useServices } from '@/composables/useServices'
import type { ServiceDef } from '@/types/service'

const props = defineProps<{ open: boolean; def: ServiceDef; suggestion?: number | null }>()
const emit = defineEmits<{ close: [] }>()

const { config, setServicePort } = useConfig()
const services = useServices()

const current = computed(() => Number(props.def.ports[0]?.host ?? 0))
const inner = computed(() => props.def.ports[0]?.container ?? '')
const overridden = computed(() => config.value.servicePorts?.[props.def.id] != null)
const value = ref('')
const check = ref<{ free: boolean; reason: string } | null>(null)
const saving = ref(false)
const error = ref('')

watch(() => props.open, (o) => {
  if (!o) return
  value.value = String(props.suggestion ?? current.value)
  error.value = ''
})

let timer: ReturnType<typeof setTimeout> | undefined
watch(value, (raw) => {
  clearTimeout(timer)
  check.value = null
  const n = Number(raw)
  if (!Number.isInteger(n) || n < 1024 || n > 65535) {
    check.value = { free: false, reason: 'Use a port from 1024 to 65535.' }
    return
  }
  if (n === current.value) return
  timer = setTimeout(async () => {
    try {
      check.value = await invoke<{ free: boolean; reason: string }>('project_port_check', { port: n, taken: [] })
    } catch {
      check.value = null
    }
  }, 250)
})

const valid = computed(() => {
  const n = Number(value.value)
  return Number.isInteger(n) && n >= 1024 && n <= 65535 && n !== current.value && check.value?.free !== false
})

/** Contoh baris .env dengan port baru. */
const preview = computed(() => `${props.def.id.toUpperCase()}_PORT=${value.value || '…'}`)

async function apply(port: number | null): Promise<void> {
  saving.value = true
  error.value = ''
  try {
    await setServicePort(props.def.id, port)
    await services.reloadDefinitions()
    services.dismissPortConflicts()
    await services.restart(props.def.id)
    emit('close')
  } catch (err) {
    error.value = String(err)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UiModal :open="open" :title="`Change ${def.name} port`" size="sm" @close="emit('close')">
    <p class="spm-text">The port on this PC. Inside the container {{ def.name }} keeps {{ inner }}. Kova recreates the container on start; its data volume stays.</p>
    <UiInput
      v-model="value"
      label="Port on this PC"
      mono
      :error="check && !check.free ? check.reason : undefined"
      :help="check?.free ? `${value} is free` : undefined"
      data-autofocus
      @keydown.enter="valid && apply(Number(value))"
    />
    <p class="spm-text">Connect values and Copy .env use the new port. Projects that already have {{ current }} in their .env or Environment tab need the new value, e.g. <code class="spm-mono">{{ preview }}</code>.</p>
    <p v-if="error" class="spm-err" role="alert">{{ error }}</p>
    <template #footer>
      <UiButton v-if="overridden" variant="quiet" size="md" :disabled="saving" @click="apply(null)">Use the default port</UiButton>
      <UiButton size="md" @click="emit('close')">Cancel</UiButton>
      <UiButton variant="primary" size="md" :disabled="!valid" :loading="saving" @click="apply(Number(value))">Change port and start</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.spm-text { margin: 0; color: var(--muted); font-size: var(--text-sm); }
.spm-mono { font-family: var(--font-mono); font-size: var(--text-xs); }
.spm-err { margin: 0; color: var(--red); font-size: var(--text-sm); }
</style>
