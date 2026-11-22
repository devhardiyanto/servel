<script setup lang="ts">
// Tombol ⋯ dengan menu aksi. Panah atas/bawah berpindah, Enter menjalankan,
// Esc menutup dan mengembalikan fokus ke tombol. Item destruktif diletakkan
// terakhir oleh pemanggil; `separator` menggambar garis di atas item.
// Item dengan `children` membuka submenu di tempat yang sama (panah kanan atau
// Enter); panah kiri atau "Back" kembali. Lebih mudah dipakai dengan keyboard
// dan pembaca layar daripada submenu melayang di samping.
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'

export interface MenuItem {
  id: string
  label: string
  hint?: string
  danger?: boolean
  disabled?: boolean
  separator?: boolean
  children?: MenuItem[]
}

const props = defineProps<{ items: MenuItem[]; label: string }>()
const emit = defineEmits<{ select: [id: string] }>()

const open = ref(false)
const trigger = ref<HTMLButtonElement | null>(null)
const menu = ref<HTMLElement | null>(null)
const pos = ref({ top: 0, left: 0 })
/** Item induk submenu yang sedang terbuka; null = menu utama. */
const sub = ref<MenuItem | null>(null)
const shown = computed<MenuItem[]>(() =>
  sub.value ? [{ id: '__back', label: `‹ ${sub.value.label.replace(/\s*›$/, '')}` }, ...(sub.value.children ?? [])] : props.items,
)

function buttons(): HTMLButtonElement[] {
  return Array.from(menu.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])
}

async function show(): Promise<void> {
  sub.value = null
  open.value = true
  await nextTick()
  const t = trigger.value?.getBoundingClientRect()
  const m = menu.value?.getBoundingClientRect()
  if (t && m) {
    // Rata kanan dengan tombol; dibalik ke atas bila tidak muat di bawah.
    const left = Math.max(8, Math.min(t.right - m.width, window.innerWidth - m.width - 8))
    const below = t.bottom + 4
    const top = below + m.height > window.innerHeight - 8 ? Math.max(8, t.top - m.height - 4) : below
    pos.value = { top, left }
  }
  buttons()[0]?.focus()
  document.addEventListener('mousedown', onOutside, true)
  window.addEventListener('resize', onResize)
}

function hide(refocus = false): void {
  if (!open.value) return
  open.value = false
  document.removeEventListener('mousedown', onOutside, true)
  window.removeEventListener('resize', onResize)
  if (refocus === true) trigger.value?.focus()
}

function onResize(): void {
  hide()
}

function onOutside(e: MouseEvent): void {
  const target = e.target as Node
  if (!menu.value?.contains(target) && !trigger.value?.contains(target)) hide()
}

function onKey(e: KeyboardEvent): void {
  const items = buttons()
  const i = items.indexOf(document.activeElement as HTMLButtonElement)
  if (e.key === 'ArrowDown') { e.preventDefault(); items[(i + 1) % items.length]?.focus() }
  else if (e.key === 'ArrowUp') { e.preventDefault(); items[(i - 1 + items.length) % items.length]?.focus() }
  else if (e.key === 'Home') { e.preventDefault(); items[0]?.focus() }
  else if (e.key === 'End') { e.preventDefault(); items[items.length - 1]?.focus() }
  else if (e.key === 'ArrowRight') {
    const it = shown.value.find((x) => `mi-${x.id}` === (document.activeElement as HTMLElement)?.dataset.mi)
    if (it?.children) { e.preventDefault(); void enter(it) }
  }
  else if (e.key === 'ArrowLeft' && sub.value) { e.preventDefault(); void back() }
  else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); hide(true) }
  else if (e.key === 'Tab') { hide() }
}

async function enter(it: MenuItem): Promise<void> {
  sub.value = it
  await nextTick()
  buttons()[1]?.focus()
}

async function back(): Promise<void> {
  const parent = sub.value
  sub.value = null
  await nextTick()
  menu.value?.querySelector<HTMLButtonElement>(`[data-mi="mi-${parent?.id}"]`)?.focus()
}

function pick(it: MenuItem): void {
  if (it.id === '__back') return void back()
  if (it.children) return void enter(it)
  hide(true)
  emit('select', it.id)
}

onBeforeUnmount(() => hide())
</script>

<template>
  <button
    ref="trigger"
    type="button"
    class="ui-menu-btn"
    :aria-label="label"
    aria-haspopup="menu"
    :aria-expanded="open"
    @click.stop="open ? hide(true) : show()"
  >
    <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8h.01M8 8h.01M12.5 8h.01" /></svg>
  </button>
  <Teleport to="body">
    <div
      v-if="open"
      ref="menu"
      class="ui-menu"
      role="menu"
      :aria-label="label"
      :style="{ top: `${pos.top}px`, left: `${pos.left}px` }"
      @keydown="onKey"
    >
      <template v-for="it in shown" :key="it.id">
        <div v-if="it.separator" class="ui-menu__sep" role="separator"></div>
        <button
          type="button"
          class="ui-menu__item"
          :class="{ 'ui-menu__item--danger': it.danger, 'ui-menu__item--back': it.id === '__back' }"
          role="menuitem"
          tabindex="-1"
          :data-mi="`mi-${it.id}`"
          :aria-haspopup="it.children ? 'menu' : undefined"
          :disabled="it.disabled"
          @click="pick(it)"
        >
          {{ it.label }}<span v-if="it.hint" class="ui-menu__hint">{{ it.hint }}</span><span v-if="it.children" class="ui-menu__hint" aria-hidden="true">›</span>
        </button>
      </template>
    </div>
  </Teleport>
</template>

<style scoped>
.ui-menu-btn {
  width: 28px;
  height: 28px;
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-xs);
  background: var(--surface2);
  color: var(--text);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease);
}
.ui-menu-btn:hover,
.ui-menu-btn[aria-expanded='true'] { background: var(--surface3); }
.ui-menu-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.ui-menu-btn svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 2.5; stroke-linecap: round; }
.ui-menu {
  position: fixed;
  z-index: 55;
  min-width: 232px;
  display: flex;
  flex-direction: column;
  padding: 4px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface2);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
}
.ui-menu__item {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 30px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--radius-xs);
  background: none;
  color: var(--text);
  font: 400 var(--text-sm) var(--font-sans);
  text-align: left;
  cursor: pointer;
}
.ui-menu__item:hover:not(:disabled),
.ui-menu__item:focus-visible { background: var(--surface3); outline: none; }
.ui-menu__item:disabled { color: var(--faint); cursor: not-allowed; }
.ui-menu__item--danger { color: var(--red); }
.ui-menu__item--back { color: var(--muted); }
.ui-menu__hint { margin-left: auto; font: var(--text-xs) var(--font-mono); color: var(--muted); }
.ui-menu__sep { height: 1px; margin: 4px 0; background: var(--border); }
</style>
