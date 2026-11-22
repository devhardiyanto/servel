// Lebar dan status tutup panel detail, diingat per layar. Disimpan di
// localStorage jendela ini: preferensi tampilan saja, bukan config, jadi kalau
// hilang panel kembali ke lebar bawaan. Satu state per nama, supaya palet
// perintah bisa menutup panel yang sama dengan layar Projects.
import { effectScope, ref, watch, type Ref } from 'vue'

function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* hanya untuk sesi ini */
  }
}

type PanelState = { width: Ref<number>; collapsed: Ref<boolean> }
const panels = new Map<string, PanelState>()

export function usePanelWidth(name: string, opts: { default: number; min: number; max: number }): PanelState {
  const cached = panels.get(name)
  if (cached) return cached
  const wKey = `kova.panel.${name}.width`
  const cKey = `kova.panel.${name}.collapsed`
  const saved = Number(read(wKey))
  const width = ref(Number.isFinite(saved) && saved >= opts.min && saved <= opts.max ? saved : opts.default)
  const collapsed = ref(read(cKey) === '1')
  // Scope lepas: watch tetap hidup setelah komponen pemanggil pertama dilepas.
  effectScope(true).run(() => {
    watch(width, (w) => write(wKey, String(w)))
    watch(collapsed, (c) => write(cKey, c ? '1' : '0'))
  })
  const state = { width, collapsed }
  panels.set(name, state)
  return state
}

/** Ukuran panel detail Projects; dipakai layar Projects dan palet perintah. */
export const PROJECTS_PANEL = { default: 430, min: 320, max: 640 }
