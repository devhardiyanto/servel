<script setup lang="ts">
// Ikon stroke 16×16. Dekoratif: tombol yang
// memakainya tetap punya label teks atau aria-label sendiri.
export type IconName =
  | 'plus'
  | 'play'
  | 'stop'
  | 'restart'
  | 'external'
  | 'more'
  | 'folder'
  | 'folder-plus'
  | 'terminal'
  | 'code'
  | 'copy'
  | 'download'
  | 'close'
  | 'search'
  | 'trash'

withDefaults(defineProps<{ name: IconName; size?: 14 | 16 }>(), { size: 14 })
</script>

<template>
  <svg class="ui-icon" :class="{ 'ui-icon--dots': name === 'more' }" :width="size" :height="size" viewBox="0 0 16 16" aria-hidden="true">
    <path v-if="name === 'plus'" d="M8 3v10M3 8h10" />
    <path v-else-if="name === 'play'" d="M5 3.5v9l7-4.5z" />
    <rect v-else-if="name === 'stop'" x="4" y="4" width="8" height="8" rx="1.5" />
    <path v-else-if="name === 'restart'" d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5V5h-2.5" />
    <path v-else-if="name === 'external'" d="M9 3h4v4M13 3L7.5 8.5M12 9.5V13H3V4h3.5" />
    <path v-else-if="name === 'more'" d="M3.5 8h.01M8 8h.01M12.5 8h.01" />
    <path v-else-if="name === 'folder-plus'" d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5zM8 7v4M6 9h4" />
    <path v-else-if="name === 'folder'" d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
    <template v-else-if="name === 'terminal'"><rect x="2" y="3" width="12" height="10" rx="1.5" /><path d="M5 6.5l2 1.5-2 1.5M8.5 10H11" /></template>
    <path v-else-if="name === 'code'" d="M6 4.5L2.5 8 6 11.5M10 4.5L13.5 8 10 11.5" />
    <template v-else-if="name === 'copy'"><rect x="5" y="5" width="8" height="8" rx="1.5" /><path d="M3 10.5V3h7.5" /></template>
    <path v-else-if="name === 'download'" d="M8 2.5v8M4.5 7L8 10.5 11.5 7M3 13.5h10" />
    <path v-else-if="name === 'close'" d="M4 4l8 8M12 4l-8 8" />
    <template v-else-if="name === 'search'"><circle cx="7" cy="7" r="4" /><path d="M10 10l3.5 3.5" /></template>
    <path v-else-if="name === 'trash'" d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5" />
  </svg>
</template>

<style scoped>
.ui-icon {
  flex: none;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.ui-icon--dots { stroke-width: 2.5; }
</style>
