<script setup lang="ts">
// "Port N is used by another program": siapa pemiliknya (nama + PID), dan
// pilihan Use <port lain> atau Stop <program>… (dengan konfirmasi). Dipakai
// panel detail project dan service.
import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiModal from '@/components/ui/UiModal.vue'

interface PortOwner {
  pid: number
  name: string
  path: string | null
  command: string | null
  started: string | null
}

const props = defineProps<{
  port: number
  /** Nama yang menunggu port ini (project atau service). */
  subject: string
  /** Port pengganti yang diusulkan; kosong = tombol tidak tampil. */
  suggestion?: number | null
  suggestionLabel?: string
}>()
const emit = defineEmits<{ usePort: [port: number]; stopped: []; close: [] }>()

const owner = ref<PortOwner | null>(null)
const loading = ref(true)
const confirming = ref(false)
const stopping = ref(false)
const stopError = ref('')

async function load(): Promise<void> {
  loading.value = true
  try {
    owner.value = await invoke<PortOwner | null>('port_owner', { port: props.port })
  } catch {
    owner.value = null
  } finally {
    loading.value = false
  }
}
onMounted(load)
watch(() => props.port, load)

const who = computed(() => (owner.value ? `${owner.value.name} (PID ${owner.value.pid})` : 'Another program'))

async function stop(): Promise<void> {
  if (!owner.value) return
  stopping.value = true
  stopError.value = ''
  try {
    await invoke('port_owner_stop', { pid: owner.value.pid, started: owner.value.started })
    confirming.value = false
    emit('stopped')
  } catch (err) {
    stopError.value = String(err)
  } finally {
    stopping.value = false
  }
}
</script>

<template>
  <UiBanner tone="err" :title="`Port ${port} is used by another program`">
    <template v-if="loading">Checking which program uses {{ port }}…</template>
    <template v-else>
      <code class="pob-mono">{{ who }}</code> is listening on {{ port }}. It was not started by Kova.
      <span v-if="!owner"> Kova cannot see programs of other users or of Administrator.</span>
    </template>
    <template #actions>
      <UiButton v-if="suggestion" variant="accent" @click="emit('usePort', suggestion)">{{ suggestionLabel ?? `Use ${suggestion}` }}</UiButton>
      <UiButton v-if="owner" variant="danger" @click="confirming = true">Stop {{ owner.name }}…</UiButton>
      <UiButton variant="quiet" @click="load">Check again</UiButton>
    </template>
  </UiBanner>

  <UiModal :open="confirming" :title="`Stop ${owner?.name ?? 'program'}?`" size="sm" danger @close="confirming = false">
    <p class="pob-text">
      This program was not started by Kova. If it is another dev server or an editor tool, unsaved work in it can be lost.
      After it stops, Kova starts {{ subject }}.
    </p>
    <dl v-if="owner" class="pob-kv">
      <dt>Program</dt><dd class="pob-mono">{{ owner.path ?? owner.name }}</dd>
      <template v-if="owner.command"><dt>Command</dt><dd class="pob-mono">{{ owner.command }}</dd></template>
      <dt>Process id</dt><dd class="pob-mono">{{ owner.pid }}</dd>
    </dl>
    <p v-if="stopError" class="pob-err" role="alert">{{ stopError }}</p>
    <template #footer>
      <UiButton size="md" data-autofocus @click="confirming = false">Cancel</UiButton>
      <UiButton size="md" variant="danger" :loading="stopping" @click="stop">Stop program and start {{ subject }}</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.pob-mono { font-family: var(--font-mono); font-size: var(--text-xs); word-break: break-all; }
.pob-text { margin: 0 0 var(--space-3); color: var(--muted); font-size: var(--text-sm); }
.pob-kv { display: grid; grid-template-columns: 96px minmax(0, 1fr); gap: var(--space-2) var(--space-3); margin: 0; font-size: var(--text-sm); }
.pob-kv dt { color: var(--muted); }
.pob-kv dd { margin: 0; min-width: 0; }
.pob-err { margin: var(--space-3) 0 0; color: var(--red); font-size: var(--text-sm); }
</style>
