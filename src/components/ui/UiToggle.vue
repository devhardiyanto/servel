<script setup lang="ts">
// Switch on/off (role="switch"). Komponen hanya menampilkan state; induk
// mengubah nilainya lewat @click supaya aksi async (simpan config, ubah
// php.ini) tetap di tangan induk.
withDefaults(
  defineProps<{
    on: boolean
    size?: 'sm' | 'md'
    label?: string
    disabled?: boolean
  }>(),
  { size: 'md', label: undefined, disabled: false },
)
</script>

<template>
  <button
    type="button"
    class="ui-toggle"
    :class="[`ui-toggle--${size}`, { 'ui-toggle--on': on }]"
    role="switch"
    :aria-checked="on"
    :aria-label="label"
    :disabled="disabled"
  >
    <span class="ui-toggle__knob"></span>
  </button>
</template>

<style scoped>
.ui-toggle {
  --w: 36px;
  --h: 20px;
  --k: 14px;
  position: relative;
  flex: none;
  width: var(--w);
  height: var(--h);
  padding: 0;
  border-radius: var(--radius-md);
  background: var(--surface2);
  border: 1px solid var(--border);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease);
}
.ui-toggle--sm { --w: 32px; --h: 18px; --k: 12px; }
.ui-toggle--on {
  background: color-mix(in srgb, var(--accent) 35%, transparent);
  border-color: color-mix(in srgb, var(--accent) 60%, transparent);
}
.ui-toggle:disabled { cursor: wait; opacity: 0.6; }
.ui-toggle:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.ui-toggle__knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: var(--k);
  height: var(--k);
  border-radius: 50%;
  background: var(--muted);
  transition: transform var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
}
.ui-toggle--on .ui-toggle__knob {
  transform: translateX(calc(var(--w) - var(--k) - 6px));
  background: var(--accent);
}
</style>
