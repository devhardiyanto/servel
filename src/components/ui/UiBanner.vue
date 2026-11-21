<script setup lang="ts">
// Ikon + judul + pesan + aksi (slot). Error memakai role="alert"; lainnya
// role="status". Ikon berbeda per jenis, jadi warna bukan satu-satunya penanda.
// v3.8: `pending` = proses sedang berjalan (ikon berputar, warna info).
defineProps<{ tone: 'info' | 'warn' | 'err' | 'ok' | 'pending'; title: string }>()
</script>

<template>
  <div class="ui-banner" :class="`ui-banner--${tone}`" :role="tone === 'err' ? 'alert' : 'status'">
    <svg class="ui-banner__icon" viewBox="0 0 16 16" aria-hidden="true">
      <path v-if="tone === 'pending'" class="ui-banner__spin" d="M8 2.5a5.5 5.5 0 1 1-5.5 5.5" />
      <path v-else-if="tone === 'info'" d="M8 2.5v8M4.5 7L8 10.5 11.5 7M3 13.5h10" />
      <path v-else-if="tone === 'warn'" d="M8 2.5L14 13H2zM8 7v2.5M8 11.2h.01" />
      <template v-else-if="tone === 'err'"><circle cx="8" cy="8" r="6" /><path d="M6 6l4 4M10 6l-4 4" /></template>
      <path v-else d="M3.5 8.5l3 3 6-7" />
    </svg>
    <div class="ui-banner__body">
      <span class="ui-banner__title">{{ title }}</span>
      <span v-if="$slots.default" class="ui-banner__msg"><slot /></span>
    </div>
    <div v-if="$slots.actions" class="ui-banner__actions"><slot name="actions" /></div>
  </div>
</template>

<style scoped>
.ui-banner {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: flex-start;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface2);
  font-size: var(--text-sm);
}
.ui-banner__icon {
  width: 16px;
  height: 16px;
  flex: none;
  margin-top: 2px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.ui-banner__body { flex: 1 1 200px; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
.ui-banner__title { font-weight: 600; color: var(--text); }
.ui-banner__msg { color: var(--muted); overflow-wrap: anywhere; }
.ui-banner__actions { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; flex: none; margin-left: auto; }
/* Di wadah sempit (panel detail) aksi turun ke bawah teks, sejajar judul. */
.ui-banner { container-type: inline-size; }
@container (max-width: 480px) {
  .ui-banner__actions { flex-basis: 100%; margin-left: 26px; }
}
.ui-banner--info { background: color-mix(in srgb, var(--accent) 7%, transparent); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
.ui-banner--info .ui-banner__icon { color: var(--accent); }
.ui-banner--warn { background: color-mix(in srgb, var(--amber) 7%, transparent); border-color: color-mix(in srgb, var(--amber) 30%, transparent); }
.ui-banner--warn .ui-banner__icon { color: var(--amber); }
.ui-banner--err { background: color-mix(in srgb, var(--red) 7%, transparent); border-color: color-mix(in srgb, var(--red) 32%, transparent); }
.ui-banner--err .ui-banner__icon { color: var(--red); }
.ui-banner--pending { background: color-mix(in srgb, var(--accent) 7%, transparent); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
.ui-banner--pending .ui-banner__icon { color: var(--accent); }
.ui-banner__spin { transform-origin: 8px 8px; animation: ui-banner-spin 0.9s linear infinite; }
@keyframes ui-banner-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .ui-banner__spin { animation: none; } }
.ui-banner--ok { background: color-mix(in srgb, var(--green) 7%, transparent); border-color: color-mix(in srgb, var(--green) 30%, transparent); }
.ui-banner--ok .ui-banner__icon { color: var(--green); }
</style>
