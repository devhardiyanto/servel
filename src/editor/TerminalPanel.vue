<script setup lang="ts">
// Panel terminal Kova Editor (spike 3): beberapa terminal PowerShell di
// folder jendela ini. xterm.js dimuat saat terminal pertama dibuka.
import { nextTick, onBeforeUnmount, onMounted, ref, shallowRef, triggerRef } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import type { Terminal } from '@xterm/xterm'
import type { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { CODE_FONT } from './theme'
import { errorText } from './api'

const props = defineProps<{ folderName: string }>()
const emit = defineEmits<{ hide: []; error: [string] }>()

interface Tab { key: number; id: number | null; term: Terminal; fit: FitAddon; el: HTMLElement; exited: boolean; title: string }
const tabs = shallowRef<Tab[]>([])
const current = ref(0)
const host = ref<HTMLElement | null>(null)
let seq = 0

/** Pesan pty: ArrayBuffer (byte keluaran) atau `{ exit }` saat shell keluar. */
type Msg = ArrayBuffer | Uint8Array | number[] | { exit: number | null }
function bytes(m: Msg): Uint8Array | null {
  if (m instanceof ArrayBuffer) return new Uint8Array(m)
  if (m instanceof Uint8Array) return m
  if (Array.isArray(m)) return Uint8Array.from(m)
  return null
}

async function add(): Promise<void> {
  const [{ Terminal }, { FitAddon }] = await Promise.all([import('@xterm/xterm'), import('@xterm/addon-fit')])
  await nextTick()
  if (!host.value) return
  const css = getComputedStyle(document.documentElement)
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback
  const term = new Terminal({
    fontFamily: CODE_FONT,
    fontSize: 13,
    cursorBlink: true,
    scrollback: 5000,
    theme: { background: v('--bg', '#15171b'), foreground: v('--text', '#e9eaee'), cursor: v('--text', '#e9eaee'), selectionBackground: 'rgba(111,195,234,.3)' },
  })
  const fit = new FitAddon()
  term.loadAddon(fit)
  const el = document.createElement('div')
  el.className = 'kt-term'
  host.value.appendChild(el)
  term.open(el)
  const tab: Tab = { key: ++seq, id: null, term, fit, el, exited: false, title: `${tabs.value.length + 1}: ${props.folderName}` }
  tabs.value = [...tabs.value, tab]
  show(tabs.value.length - 1)
  // Ctrl C menyalin bila ada teks terpilih (seperti VS Code); tanpa pilihan
  // diteruskan ke shell sebagai interupsi.
  term.attachCustomKeyEventHandler((e) => {
    if (e.type === 'keydown' && e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === 'c' && term.hasSelection()) {
      void navigator.clipboard.writeText(term.getSelection())
      term.clearSelection()
      return false
    }
    return true
  })
  const ch = new Channel<Msg>()
  ch.onmessage = (m) => {
    const b = bytes(m)
    if (b) term.write(b)
    else {
      tab.exited = true
      triggerRef(tabs)
      term.write('\r\n\x1b[2mThe shell ended. Close this terminal or open a new one.\x1b[0m\r\n')
    }
  }
  try {
    tab.id = await invoke<number>('term_open', { cols: term.cols, rows: term.rows, onData: ch })
  } catch (e) {
    emit('error', `Couldn't open a terminal: ${errorText(e)}`)
    close(tabs.value.indexOf(tab))
    return
  }
  term.onData((data) => { if (tab.id !== null && !tab.exited) void invoke('term_write', { id: tab.id, data }).catch(() => {}) })
  term.onResize(({ cols, rows }) => { if (tab.id !== null && !tab.exited) void invoke('term_resize', { id: tab.id, cols, rows }).catch(() => {}) })
  term.focus()
}

function show(i: number): void {
  current.value = i
  tabs.value.forEach((t, j) => { t.el.style.display = j === i ? 'block' : 'none' })
  const t = tabs.value[i]
  if (t) requestAnimationFrame(() => { t.fit.fit(); t.term.focus() })
}

function close(i: number): void {
  const t = tabs.value[i]
  if (!t) return
  if (t.id !== null) void invoke('term_close', { id: t.id }).catch(() => {})
  t.term.dispose()
  t.el.remove()
  tabs.value = tabs.value.filter((_, j) => j !== i)
  if (!tabs.value.length) {
    emit('hide')
    return
  }
  show(Math.min(i, tabs.value.length - 1))
}

/** Dipanggil saat panel ditampilkan: buka terminal pertama atau fokuskan yang ada. */
function reveal(): void {
  if (!tabs.value.length) void add()
  else show(current.value)
}
defineExpose({ reveal, add })

let ro: ResizeObserver | null = null
onMounted(() => {
  ro = new ResizeObserver(() => tabs.value[current.value]?.fit.fit())
  if (host.value) ro.observe(host.value)
})
onBeforeUnmount(() => {
  ro?.disconnect()
  for (const t of tabs.value) {
    if (t.id !== null) void invoke('term_close', { id: t.id }).catch(() => {})
    t.term.dispose()
  }
})
</script>

<template>
  <section class="kt" aria-label="Terminal">
    <div class="kt-head">
      <span class="kt-label">Terminal</span>
      <div class="kt-tabs" role="tablist">
        <button
          v-for="(t, i) in tabs" :key="t.key" type="button" role="tab" class="kt-tab" :class="{ on: i === current, ended: t.exited }"
          :aria-selected="i === current" @click="show(i)"
        >{{ t.title }}<span v-if="t.exited"> · ended</span></button>
      </div>
      <span class="kt-sp"></span>
      <button type="button" class="kt-icon" aria-label="New terminal" title="New terminal" @click="add">+</button>
      <button v-if="tabs.length" type="button" class="kt-icon" aria-label="Close this terminal" title="Close this terminal" @click="close(current)"><svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5"/></svg></button>
      <button type="button" class="kt-icon" aria-label="Hide panel" title="Hide panel (Ctrl `)" @click="emit('hide')">×</button>
    </div>
    <div ref="host" class="kt-host"></div>
  </section>
</template>

<style scoped>
.kt { height: 100%; display: flex; flex-direction: column; border-top: 1px solid var(--border); background: var(--bg); }
.kt-head { height: 32px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 8px 0 14px; border-bottom: 1px solid var(--border); font-size: 12px; }
.kt-label { color: var(--text); font-weight: 600; }
.kt-tabs { display: flex; gap: 2px; overflow-x: auto; }
.kt-tab { height: 24px; padding: 0 8px; border: 0; border-radius: 4px; background: none; color: var(--muted); font: 12px var(--font-sans); cursor: pointer; white-space: nowrap; }
.kt-tab.on { background: var(--surface2); color: var(--text); }
.kt-tab.ended { color: var(--faint); }
.kt-tab:focus-visible, .kt-icon:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.kt-sp { flex: 1; }
.kt-icon { width: 26px; height: 24px; border: 0; border-radius: 4px; background: none; color: var(--muted); cursor: pointer; font-size: 14px; }
.kt-icon:hover { background: var(--surface2); color: var(--text); }
.kt-host { flex: 1; min-height: 0; padding: 4px 0 0 10px; }
.kt-host :deep(.kt-term) { height: 100%; }
</style>
