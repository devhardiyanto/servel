<script setup lang="ts">
// Ikon · judul (apa yang kosong) · satu kalimat (apa yang akan terjadi) · aksi.
defineProps<{ title: string; icon?: 'folder' | 'database' | 'lines' | 'search' }>()
</script>

<template>
  <div class="ui-empty">
    <div class="ui-empty__ico" aria-hidden="true">
      <svg viewBox="0 0 16 16">
        <path v-if="icon === 'database'" d="M3 4c0-1.1 2.2-2 5-2s5 .9 5 2-2.2 2-5 2-5-.9-5-2zM3 4v8c0 1.1 2.2 2 5 2s5-.9 5-2V4" />
        <path v-else-if="icon === 'lines'" d="M3 3.5h10M3 6.5h10M3 9.5h7M3 12.5h5" />
        <template v-else-if="icon === 'search'"><circle cx="7" cy="7" r="4" /><path d="M10 10l3.5 3.5" /></template>
        <path v-else d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
      </svg>
    </div>
    <h3>{{ title }}</h3>
    <p v-if="$slots.default"><slot /></p>
    <div v-if="$slots.actions" class="ui-empty__acts"><slot name="actions" /></div>
  </div>
</template>

<style scoped>
.ui-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 40px;
  text-align: center;
}
.ui-empty__ico {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-md);
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface2);
  color: var(--muted);
}
.ui-empty__ico svg { width: 22px; height: 22px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
.ui-empty h3 { margin: 6px 0 0; font-size: var(--text-lg); font-weight: 600; color: var(--text); }
.ui-empty p { margin: 0; max-width: 400px; color: var(--muted); font-size: var(--text-sm); }
.ui-empty__acts { display: flex; gap: 8px; margin-top: 8px; }
</style>
