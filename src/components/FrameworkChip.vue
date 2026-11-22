<script setup lang="ts">
// Chip jenis project dengan logo framework. Garis dan latar diwarnai merek
// (45% / 12%), teks tetap warna teks biasa supaya kontrasnya tidak bergantung
// pada merek. Jenis tanpa logo memakai chip abu-abu biasa.
import { computed } from 'vue'
import UiChip from '@/components/ui/UiChip.vue'
import FrameworkLogo from '@/components/ui/FrameworkLogo.vue'
import { frameworkColor, frameworkIcon } from '@/utils/framework'

const props = defineProps<{ kind: string }>()
const icon = computed(() => frameworkIcon(props.kind))
</script>

<template>
  <span v-if="icon" class="fw-chip" :style="{ '--fw': frameworkColor(icon) }">
    <FrameworkLogo :kind="kind" :size="12" />{{ kind }}
  </span>
  <UiChip v-else>{{ kind }}</UiChip>
</template>

<style scoped>
.fw-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 22px;
  padding: 0 8px;
  border-radius: var(--radius-xs);
  border: 1px solid color-mix(in srgb, var(--fw) 45%, transparent);
  background: color-mix(in srgb, var(--fw) 12%, transparent);
  color: var(--text);
  font-size: var(--text-xs);
  white-space: nowrap;
}
</style>
