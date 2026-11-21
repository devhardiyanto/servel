<script setup lang="ts">
// Hasil auto-start saat Kova dibuka (v3.7.5). Dulu hanya tercatat di Logs,
// jadi service yang gagal atau dilewati tidak terlihat.
// - Menunggu engine / sedang start: banner info.
// - Berhasil: toast singkat, banner hilang.
// - Dilewati / gagal: banner dengan alasan, Retry, dan (konflik port) Open Services.
import { computed, inject, watch } from 'vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { useAutoStart } from '@/composables/useAutoStart'
import { useToast } from '@/composables/useToast'
import { SetViewKey } from '@/types/navigation'

const { state, retry, dismiss } = useAutoStart()
const toast = useToast()
const setView = inject(SetViewKey, () => {})

const busy = computed(() => state.value.phase === 'waiting-engine' || state.value.phase === 'starting')
const problem = computed(() => state.value.phase === 'skipped' || state.value.phase === 'failed')

watch(
  () => state.value.phase,
  (phase) => {
    if (phase === 'done') {
      toast.show(state.value.message)
      dismiss()
    }
  },
)

function openSettings(): void {
  setView('settings')
  dismiss()
}

function openServices(): void {
  setView('services')
  dismiss()
}
</script>

<template>
  <div v-if="busy || problem" class="ub as">
    <UiBanner v-if="busy" tone="info" :title="state.message" />
    <UiBanner v-else :tone="state.phase === 'failed' ? 'err' : 'warn'" :title="state.message">
      <template v-if="state.detail && state.phase === 'failed'">{{ state.detail }}</template>
      <template #actions>
        <UiButton v-if="state.portConflict" variant="quiet" @click="openServices">Open Services</UiButton>
        <UiButton v-if="state.engineProblem" variant="quiet" @click="openSettings">Engine settings</UiButton>
        <UiButton variant="primary" @click="retry">Retry</UiButton>
        <UiButton variant="quiet" aria-label="Dismiss" @click="dismiss">&times;</UiButton>
      </template>
    </UiBanner>
  </div>
</template>

<style scoped>
.as { padding: var(--space-3) var(--space-6) 0; flex: none !important; }
</style>
