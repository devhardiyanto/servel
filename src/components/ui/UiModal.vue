<script setup lang="ts">
// Dialog di tengah layar. Fokus terkunci di dalam, Esc menutup, fokus kembali
// ke pemicu. `danger` = konfirmasi destruktif (role alertdialog); fokus awal
// jatuh ke elemen bertanda `data-autofocus` (Cancel untuk aksi destruktif).
import { nextTick, onBeforeUnmount, ref, useId, watch } from 'vue'
import { focusables, trapTab } from '@/utils/focusTrap'

const props = withDefaults(defineProps<{ open: boolean; title: string; size?: 'sm' | 'md'; danger?: boolean }>(), {
  size: 'md',
  danger: false,
})
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
      const preferred = root.querySelector<HTMLElement>('[data-autofocus]')
      ;(preferred ?? focusables(root)[0] ?? root).focus()
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
    <div v-if="open" class="ui-scrim" @mousedown.self="emit('close')">
      <div
        ref="panel"
        class="ui-modal"
        :class="`ui-modal--${size}`"
        :role="danger ? 'alertdialog' : 'dialog'"
        aria-modal="true"
        :aria-labelledby="titleId"
        tabindex="-1"
        @keydown="onKey"
      >
        <div class="ui-modal__head"><h2 :id="titleId">{{ title }}</h2></div>
        <div class="ui-modal__body"><slot /></div>
        <div v-if="$slots.footer" class="ui-modal__foot"><slot name="footer" /></div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ui-scrim {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-4);
  background: rgba(8, 9, 11, 0.62);
}
.ui-modal {
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 32px);
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background: var(--surface);
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55);
  outline: none;
}
.ui-modal--md { width: min(540px, 100%); }
.ui-modal--sm { width: min(440px, 100%); }
.ui-modal__head { padding: 18px 20px 6px; }
.ui-modal__head h2 { margin: 0; font-size: var(--text-lg); font-weight: 600; color: var(--text); }
.ui-modal__body {
  padding: 12px 20px 16px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  overflow-y: auto;
  font-size: var(--text-sm);
  color: var(--muted);
  line-height: 1.55;
}
.ui-modal__foot {
  padding: 12px 20px;
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  border-top: 1px solid var(--border);
}
</style>
