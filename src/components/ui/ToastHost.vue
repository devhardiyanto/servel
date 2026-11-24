<script setup lang="ts">
// Tempat toast muncul: kanan bawah, di atas status bar. Dipasang sekali di App.
import { useToast } from '@/composables/useToast'

const { toasts, dismiss, hold, release } = useToast()
</script>

<template>
  <div class="ui-toasts" aria-live="polite">
    <div
      v-for="t in toasts"
      :key="t.id"
      class="ui-toast"
      :class="{ 'ui-toast--error': t.tone === 'error' }"
      :role="t.tone === 'error' ? 'alert' : 'status'"
      @mouseenter="hold(t.id)"
      @mouseleave="release(t.id)"
      @focusin="hold(t.id)"
      @focusout="release(t.id)"
    >
      <svg class="ui-toast__icon" :class="`ui-toast__icon--${t.tone}`" viewBox="0 0 16 16" aria-hidden="true">
        <path v-if="t.tone === 'ok'" d="M3.5 8.5l3 3 6-7" />
        <template v-else><circle cx="8" cy="8" r="6" /><path d="M8 7.5v3.5M8 5.2h.01" /></template>
      </svg>
      <span class="ui-toast__text">{{ t.text }}</span>
      <button v-if="t.action" type="button" class="ui-toast__action" @click="t.action.run(); t.tone === 'error' || dismiss(t.id)">
        {{ t.action.label }}
      </button>
      <button type="button" class="ui-toast__close" aria-label="Dismiss" @click="dismiss(t.id)">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.ui-toasts {
  position: fixed;
  right: 16px;
  bottom: 44px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
}
.ui-toast {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 10px;
  max-width: 420px;
  padding: 10px 10px 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface2);
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45);
  font-size: var(--text-sm);
  color: var(--text);
}
.ui-toast__icon,
.ui-toast__close svg { width: 16px; height: 16px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
.ui-toast__icon--ok { color: var(--green); }
.ui-toast__icon--info { color: var(--accent); }
.ui-toast__icon--error { color: var(--red); }
.ui-toast--error { border-color: color-mix(in srgb, var(--red) 55%, var(--border-strong)); }
.ui-toast__text { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.ui-toast__action {
  height: 26px;
  padding: 0 10px;
  border-radius: var(--radius-xs);
  border: 1px solid var(--border-strong);
  background: var(--surface3);
  color: var(--text);
  font: 500 var(--text-xs) var(--font-sans);
  cursor: pointer;
}
.ui-toast__close {
  width: 24px;
  height: 24px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 0;
  border-radius: var(--radius-xs);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.ui-toast__close:hover { background: var(--surface3); color: var(--text); }
.ui-toast__action:focus-visible,
.ui-toast__close:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
</style>
