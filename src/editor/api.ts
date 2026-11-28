// Perintah Rust untuk Kova Editor (src-tauri/src/editor). Semua path dicek
// di Rust terhadap folder milik jendela ini.
import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'

export interface DirEntry { name: string; path: string; dir: boolean }
export interface FileText {
  text: string
  eol: '\n' | '\r\n'
  bom: boolean
  mtime: number
  size: number
  large: boolean
  readonly: boolean
}

export const editorRoot = () => invoke<string>('editor_root')
export const listDir = (path: string) => invoke<DirEntry[]>('editor_list_dir', { path })
export const indexFiles = () => invoke<string[]>('editor_index')
export const readFile = (path: string) => invoke<FileText>('editor_read', { path })
export const writeFile = (path: string, text: string, eol: string, bom: boolean, expectedMtime: number | null) =>
  invoke<number>('editor_write', { path, text, eol, bom, expectedMtime })
export const statFile = (path: string) => invoke<number | null>('editor_stat', { path })
export const showMain = () => invoke<void>('editor_show_main')
export const openEditor = (folder: string) => invoke<string>('editor_open', { folder })

/** Kode error dari Rust ("conflict: …" → "conflict"). */
export function errorCode(e: unknown): string {
  const m = /^([a-z-]+):/.exec(String(e))
  return m ? m[1] : ''
}
export function errorText(e: unknown): string {
  return String(e).replace(/^[a-z-]+:\s*/, '')
}

/** Spike v4.0: tombol Open in Kova Editor hanya tampil bila dinyalakan di Settings. */
const FLAG = 'kova.editorSpike'
function readFlag(): boolean {
  try { return localStorage.getItem(FLAG) === '1' } catch { return false }
}
export const editorSpike = ref(readFlag())
export function setEditorSpike(on: boolean): void {
  editorSpike.value = on
  try { localStorage.setItem(FLAG, on ? '1' : '0') } catch { /* tanpa storage: hanya sesi ini */ }
}
