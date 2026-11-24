<script setup lang="ts">
// Dialog "Save changes to …?" untuk useUnsavedGuard. Dipasang sekali di App.
import UiModal from '@/components/ui/UiModal.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { useUnsavedGuard } from '@/composables/useUnsavedGuard'

const { source, asking, saving, choose } = useUnsavedGuard()
</script>

<template>
  <UiModal :open="!!asking && !!source" :title="`Save changes to ${source?.label}?`" size="sm" @close="choose('stay')">
    <p>
      The Environment of {{ source?.owner }} has changes that are not saved yet.
      If you leave now without saving, they are lost.
    </p>
    <template #footer>
      <UiButton variant="quiet" class="ucm-discard" :disabled="saving" @click="choose('discard')">Discard changes</UiButton>
      <UiButton data-autofocus :disabled="saving" @click="choose('stay')">Stay here</UiButton>
      <UiButton variant="accent" :loading="saving" @click="choose('save')">Save</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.ucm-discard { margin-right: auto; color: var(--red); }
</style>
