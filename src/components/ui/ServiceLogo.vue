<script setup lang="ts">
// Decorative service logo (aria-hidden): the service name is always written
// next to it. Unknown services fall back to their first two letters.
import { computed } from 'vue'
import { SERVICE_ICONS } from '@/utils/serviceIcons'

const props = withDefaults(defineProps<{ id: string; size?: number }>(), { size: 18 })
const icon = computed(() => SERVICE_ICONS[props.id] ?? null)
</script>

<template>
  <svg
    v-if="icon && !icon.stroke"
    class="svc-logo"
    viewBox="0 0 24 24"
    :width="size"
    :height="size"
    aria-hidden="true"
    focusable="false"
    :style="{ color: icon.color ?? 'var(--text)' }"
  ><path :d="icon.path" /></svg>
  <svg
    v-else-if="icon"
    class="svc-logo svc-logo--line"
    viewBox="0 0 16 16"
    :width="size"
    :height="size"
    aria-hidden="true"
    focusable="false"
  ><path :d="icon.path" /></svg>
  <span v-else class="svc-logo svc-logo--text" aria-hidden="true" :style="{ fontSize: `${Math.round(size * 0.6)}px` }">{{ id.slice(0, 2).toUpperCase() }}</span>
</template>

<style scoped>
.svc-logo { flex: none; fill: currentColor; }
.svc-logo--line { fill: none; stroke: var(--muted); stroke-width: 1.3; stroke-linecap: round; stroke-linejoin: round; }
.svc-logo--text { font-weight: 600; color: var(--muted); line-height: 1; }
</style>
