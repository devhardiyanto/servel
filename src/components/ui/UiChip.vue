<script setup lang="ts">
// Chip = atribut (jenis, sumber, scope). `badge` = angka ringkas.
withDefaults(
  defineProps<{ tone?: 'neutral' | 'accent' | 'ok' | 'warn' | 'err'; badge?: boolean }>(),
  { tone: 'neutral', badge: false },
)
</script>

<template>
  <span :class="[badge ? 'ui-badge' : 'ui-chip', `ui-tone--${tone}`]"><slot /></span>
</template>

<style scoped>
.ui-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 22px;
  padding: 0 8px;
  border-radius: var(--radius-xs);
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--muted);
  font-size: var(--text-xs);
  white-space: nowrap;
}
.ui-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: var(--radius-xs);
  background: var(--surface3);
  color: var(--text);
  font: 500 var(--text-xs) var(--font-mono);
}
.ui-chip.ui-tone--accent { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 35%, transparent); background: color-mix(in srgb, var(--accent) 8%, transparent); }
.ui-chip.ui-tone--ok { color: var(--green); border-color: color-mix(in srgb, var(--green) 35%, transparent); background: color-mix(in srgb, var(--green) 8%, transparent); }
.ui-chip.ui-tone--warn { color: var(--amber); border-color: color-mix(in srgb, var(--amber) 35%, transparent); background: color-mix(in srgb, var(--amber) 8%, transparent); }
.ui-chip.ui-tone--err { color: var(--red); border-color: color-mix(in srgb, var(--red) 35%, transparent); background: color-mix(in srgb, var(--red) 8%, transparent); }
.ui-badge.ui-tone--err { background: color-mix(in srgb, var(--red) 16%, transparent); color: var(--red); }
.ui-badge.ui-tone--warn { background: color-mix(in srgb, var(--amber) 16%, transparent); color: var(--amber); }
</style>
