<script setup lang="ts">
// Satu komponen tombol untuk seluruh aplikasi (milestone B).
// - default: netral (surface2)
// - accent:  aksen lembut, aksi utama di dalam panel
// - primary: aksen solid, satu aksi utama per layar (onboarding, prasyarat)
// - success / danger: start / stop, hapus
// - quiet:   transparan, aksi sekunder
type Variant = 'default' | 'accent' | 'primary' | 'success' | 'danger' | 'quiet'
type Size = 'sm' | 'md' | 'lg'

withDefaults(
  defineProps<{
    variant?: Variant
    size?: Size
    type?: 'button' | 'submit'
    loading?: boolean
    disabled?: boolean
  }>(),
  { variant: 'default', size: 'sm', type: 'button', loading: false, disabled: false },
)
</script>

<template>
  <button
    :type="type"
    class="ui-btn"
    :class="[`ui-btn--${variant}`, `ui-btn--${size}`]"
    :disabled="disabled || loading"
    :aria-busy="loading || undefined"
  >
    <span v-if="loading" class="ui-btn__spinner" aria-hidden="true"></span>
    <slot />
  </button>
</template>

<style scoped>
.ui-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  font-family: var(--font-sans);
  line-height: var(--leading-tight);
  white-space: nowrap;
  border: 1px solid transparent;
  border-radius: var(--radius-xs);
  cursor: pointer;
  transition:
    background var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease),
    opacity var(--dur-fast) var(--ease);
}
.ui-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.ui-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Sizes */
.ui-btn--sm { font-size: var(--text-xs); padding: 4px 12px; }
.ui-btn--md { font-size: var(--text-sm); font-weight: 500; padding: 6px 14px; border-radius: var(--radius-sm); }
.ui-btn--lg { font-size: var(--text-md); font-weight: 500; padding: var(--space-2) var(--space-6); border-radius: var(--radius-sm); }

/* Variants */
.ui-btn--default {
  background: var(--surface2);
  border-color: var(--border-strong);
  color: var(--text);
}
.ui-btn--default:hover:not(:disabled) { background: var(--surface3); }

.ui-btn--quiet {
  background: transparent;
  border-color: var(--border-strong);
  color: var(--muted);
}
.ui-btn--quiet:hover:not(:disabled) { color: var(--text); background: color-mix(in srgb, var(--text) 5%, transparent); }

.ui-btn--accent,
.ui-btn--success,
.ui-btn--danger {
  --tone: var(--accent);
  background: color-mix(in srgb, var(--tone) 12%, transparent);
  border-color: color-mix(in srgb, var(--tone) 42%, transparent);
  color: var(--tone);
}
.ui-btn--success { --tone: var(--green); }
.ui-btn--danger { --tone: var(--red); }
.ui-btn--accent:hover:not(:disabled),
.ui-btn--success:hover:not(:disabled),
.ui-btn--danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--tone) 22%, transparent);
  border-color: color-mix(in srgb, var(--tone) 65%, transparent);
}

.ui-btn--primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--bg);
}
.ui-btn--primary:hover:not(:disabled) { opacity: 0.88; }

.ui-btn__spinner {
  width: 0.9em;
  height: 0.9em;
  border: 1.5px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  animation: ui-btn-spin 0.7s linear infinite;
}
@keyframes ui-btn-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .ui-btn__spinner { animation-duration: 2s; }
}
</style>
