<script setup lang="ts">
// Hasil auto-start saat Kova dibuka, supaya service yang gagal atau dilewati
// terlihat tanpa membuka Logs.
// - Menunggu engine / sedang start: banner info.
// - Berhasil: toast singkat, banner hilang.
// - Dilewati / gagal: banner dengan alasan, Retry, dan (konflik port) Open Services.
import { computed, inject, watch } from 'vue'
import UiStatusNotice from '@/components/ui/UiStatusNotice.vue'
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
    <UiStatusNotice
      :tone="busy ? 'pending' : state.phase === 'failed' ? 'err' : 'warn'"
      :title="state.message"
      :detail="state.phase === 'failed' ? state.detail : ''"
      :retryable="problem"
      :dismissible="problem"
      @retry="retry"
      @dismiss="dismiss"
    >
      <template v-if="state.portConflict || state.engineProblem" #actions>
        <UiButton v-if="state.portConflict" variant="quiet" @click="openServices">Open Services</UiButton>
        <UiButton v-if="state.engineProblem" variant="quiet" @click="openSettings">Engine settings</UiButton>
      </template>
    </UiStatusNotice>
  </div>
</template>

<style scoped>
.as { padding: var(--space-3) var(--space-6) 0; flex: none !important; }
</style>
