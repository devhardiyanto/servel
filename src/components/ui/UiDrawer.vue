<script setup lang="ts">
// Panel dari kanan untuk edit yang lebih panjang tanpa meninggalkan daftar.
// Fokus terkunci di dalam, Esc menutup, fokus kembali ke pemicu.
import { nextTick, onBeforeUnmount, ref, useId, watch } from 'vue'
import { focusables, trapTab } from '@/utils/focusTrap'

const props = defineProps<{ open: boolean; title: string }>()
const emit = defineEmits<{ close: [] }>()

const panel = ref<HTMLElement | null>(null)
const titleId = useId()
let lastFocus: HTMLElement | null = null

watch(
  () => props.open,
  async (open) => {
    if (open) {
      lastFocus = document.activeElement as HTMLElement | null
      await nextTick()
      const root = panel.value
      if (!root) return
      // Lewati tombol Close: fokus ke kolom pertama.
      const items = focusables(root)
      ;(items.find((el) => !el.hasAttribute('data-drawer-close')) ?? items[0] ?? root).focus()
    } else {
      lastFocus?.focus?.()
      lastFocus = null
    }
  },
  { immediate: true },
)
onBeforeUnmount(() => lastFocus?.focus?.())

function onKey(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
  } else if (panel.value) {
    trapTab(panel.value, e)
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="ui-drawer">
      <div v-if="open" class="ui-drawer-scrim" @mousedown.self="emit('close')">
        <aside
          ref="panel"
          class="ui-drawer"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="titleId"
          tabindex="-1"
          @keydown="onKey"
        >
          <header class="ui-drawer__head">
            <h2 :id="titleId">{{ title }}</h2>
            <button class="ui-drawer__close" type="button" aria-label="Close" data-drawer-close @click="emit('close')">
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
            </button>
          </header>
          <div class="ui-drawer__body"><slot /></div>
          <footer v-if="$slots.footer" class="ui-drawer__foot"><slot name="footer" /></footer>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.ui-drawer-scrim { position: fixed; inset: 0; z-index: 52; background: rgba(8, 9, 11, 0.4); }
.ui-drawer {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: min(440px, 100%);
  display: flex;
  flex-direction: column;
  border-left: 1px solid var(--border-strong);
  background: var(--surface);
  box-shadow: -18px 0 48px rgba(0, 0, 0, 0.45);
  outline: none;
}
.ui-drawer__head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 14px 16px;
  border-bottom: 1px solid var(--border);
}
.ui-drawer__head h2 { flex: 1; margin: 0; font-size: var(--text-lg); font-weight: 600; color: var(--text); }
.ui-drawer__close {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 0;
  border-radius: var(--radius-xs);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.ui-drawer__close:hover { background: var(--surface2); color: var(--text); }
.ui-drawer__close:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.ui-drawer__close svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; }
.ui-drawer__body { flex: 1; min-height: 0; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 14px; }
.ui-drawer__foot { display: flex; flex-wrap: wrap; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); }
.ui-drawer-enter-active,
.ui-drawer-leave-active { transition: opacity var(--dur-base) var(--ease); }
.ui-drawer-enter-active .ui-drawer,
.ui-drawer-leave-active .ui-drawer { transition: transform var(--dur-base) var(--ease); }
.ui-drawer-enter-from,
.ui-drawer-leave-to { opacity: 0; }
.ui-drawer-enter-from .ui-drawer,
.ui-drawer-leave-to .ui-drawer { transform: translateX(24px); }
</style>
