<script setup lang="ts">
// Banner update di atas semua layar (v3.4, canvas Flow F): tersedia → unduh →
// siap restart, atau gagal. Unduhan diverifikasi dengan kunci update sebelum
// bisa dipasang; service tetap berjalan selama mengunduh.
import { computed } from 'vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { useUpdateCheck } from '@/composables/useUpdateCheck'

const u = useUpdateCheck()

function mb(bytes: number): string {
  return (bytes / 1_048_576).toFixed(1)
}

const show = u.bannerVisible

const progressText = computed(() => {
  const total = u.totalBytes.value
  const size = total ? `${mb(u.downloaded.value)} of ${mb(total)} MB` : `${mb(u.downloaded.value)} MB`
  return `${size} · checked with the update signing key before it installs`
})
</script>

<template>
  <div v-if="show" class="ub">
    <UiBanner
      v-if="u.phase.value === 'available'"
      tone="info"
      :title="`Kova ${u.latestVersion.value} is available`"
    >
      You have {{ u.currentVersion.value }}. Projects and services keep running while it downloads.
      <template #actions>
        <UiButton variant="quiet" @click="u.openReleasePage()">Release notes</UiButton>
        <UiButton variant="primary" @click="u.download()">Update</UiButton>
        <UiButton variant="quiet" aria-label="Dismiss this update" @click="u.dismiss()">&times;</UiButton>
      </template>
    </UiBanner>

    <UiBanner
      v-else-if="u.phase.value === 'downloading'"
      tone="info"
      :title="`Downloading Kova ${u.latestVersion.value}${u.installPercent.value !== null ? ` · ${u.installPercent.value}%` : ''}`"
    >
      {{ progressText }}
      <div
        class="ub-track"
        role="progressbar"
        :aria-valuenow="u.installPercent.value ?? undefined"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-label="Download progress"
      >
        <div class="ub-fill" :class="{ 'ub-fill--unknown': u.installPercent.value === null }" :style="{ width: `${u.installPercent.value ?? 30}%` }"></div>
      </div>
      <template #actions>
        <UiButton variant="quiet" @click="u.hideBanner()">Hide</UiButton>
      </template>
    </UiBanner>

    <UiBanner v-else-if="u.phase.value === 'ready'" tone="ok" :title="`Kova ${u.readyVersion.value} is ready`">
      Restart to finish, or choose Later to install it when you quit Kova. Service containers keep running; dev servers stop and start again if marked "start on launch".
      <template #actions>
        <UiButton variant="quiet" @click="u.later()">Later</UiButton>
        <UiButton variant="primary" :loading="u.installing.value" @click="u.apply()">Restart now</UiButton>
      </template>
    </UiBanner>

    <UiBanner v-else-if="u.phase.value === 'error'" tone="err" title="Update failed">
      {{ u.installError.value }} Nothing was installed; you're still on {{ u.currentVersion.value }}.
      <template #actions>
        <UiButton variant="quiet" @click="u.openReleasePage()">Open release page</UiButton>
        <UiButton @click="u.download()">Try again</UiButton>
      </template>
    </UiBanner>
  </div>
</template>

<style scoped>
.ub { padding: var(--space-3) var(--space-6) 0; flex: none !important; }
.ub-track { margin-top: var(--space-2); height: 4px; border-radius: 2px; background: var(--surface3); overflow: hidden; max-width: 360px; }
.ub-fill { height: 100%; background: var(--accent); transition: width var(--dur-fast) var(--ease); }
.ub-fill--unknown { animation: ub-slide 1.2s ease-in-out infinite; }
@keyframes ub-slide { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
@media (prefers-reduced-motion: reduce) { .ub-fill--unknown { animation: none; } }
</style>
