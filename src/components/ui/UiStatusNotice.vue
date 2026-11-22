<script setup lang="ts">
// Satu pola untuk keadaan sementara sebuah proses: sedang berjalan,
// berhasil, dilewati, atau gagal. Dipakai oleh hasil auto-start, pemeriksaan
// engine di Settings, dan error service, supaya ketiganya tampil dan berperilaku
// sama:
// - `pending`: ikon berputar, tanpa tombol.
// - `warn` / `err`: alasan di `detail`, tombol Retry (bila ada handler `retry`)
//   dan tombol tutup (bila `dismissible`). Aksi tambahan lewat slot `actions`.
// - `ok` / `info`: ringkasan; boleh ditutup.
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'

withDefaults(
  defineProps<{
    tone: 'pending' | 'ok' | 'info' | 'warn' | 'err'
    title: string
    detail?: string
    /** Tampilkan tombol Retry. */
    retryable?: boolean
    retryLabel?: string
    /** Tampilkan tombol tutup (×). */
    dismissible?: boolean
    busy?: boolean
  }>(),
  { detail: '', retryable: false, retryLabel: 'Retry', dismissible: false, busy: false },
)
const emit = defineEmits<{ retry: []; dismiss: [] }>()
</script>

<template>
  <UiBanner :tone="tone" :title="title">
    <template v-if="detail || $slots.default" #default>
      <slot>{{ detail }}</slot>
    </template>
    <template v-if="tone !== 'pending' && (retryable || dismissible || $slots.actions)" #actions>
      <slot name="actions" />
      <UiButton v-if="retryable" variant="primary" :loading="busy" @click="emit('retry')">{{ retryLabel }}</UiButton>
      <UiButton v-if="dismissible" variant="quiet" aria-label="Dismiss" @click="emit('dismiss')">&times;</UiButton>
    </template>
  </UiBanner>
</template>
