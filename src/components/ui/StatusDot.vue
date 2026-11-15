<script setup lang="ts">
// Status dengan bentuk berbeda per keadaan (bukan warna saja) dan label teks.
// Service memetakan: not_created → stopped, stopping → starting, error → exited.
export type DotState = 'running' | 'starting' | 'stopped' | 'unresponsive' | 'exited'

defineProps<{ state: DotState; label?: string }>()
</script>

<template>
  <span class="ui-st" :class="`ui-st--${state}`">
    <span class="ui-dot" :class="`ui-dot--${state}`" aria-hidden="true"></span>
    <span v-if="label">{{ label }}</span>
  </span>
</template>

<style scoped>
.ui-st { display: inline-flex; align-items: center; gap: 6px; font-size: var(--text-xs); color: var(--muted); white-space: nowrap; }
.ui-st--running { color: var(--green); }
.ui-st--starting,
.ui-st--unresponsive { color: var(--amber); }
.ui-st--exited { color: var(--red); }
.ui-dot { display: inline-block; width: 8px; height: 8px; flex: none; border-radius: 50%; }
.ui-dot--running { background: var(--green); }
.ui-dot--starting { border: 2px solid var(--amber); border-right-color: transparent; animation: ui-dot-spin 1s linear infinite; }
.ui-dot--stopped { border: 1.5px solid var(--faint); }
.ui-dot--unresponsive { background: var(--amber); box-shadow: 0 0 0 2px color-mix(in srgb, var(--amber) 30%, transparent); }
.ui-dot--exited { background: var(--red); border-radius: 2px; }
@keyframes ui-dot-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .ui-dot--starting { animation-duration: 2.4s; }
}
</style>
