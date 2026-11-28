// Bahasa per ekstensi, dimuat saat file pertama jenis itu dibuka supaya
// jendela editor tidak memuat semua grammar di awal.
import type { Extension } from '@codemirror/state'

export interface Lang { name: string; load: () => Promise<Extension> }

const PHP: Lang = { name: 'PHP', load: async () => (await import('@codemirror/lang-php')).php() }
const JS: Lang = { name: 'JavaScript', load: async () => (await import('@codemirror/lang-javascript')).javascript() }
const TS: Lang = { name: 'TypeScript', load: async () => (await import('@codemirror/lang-javascript')).javascript({ typescript: true }) }
const JSX: Lang = { name: 'JavaScript JSX', load: async () => (await import('@codemirror/lang-javascript')).javascript({ jsx: true }) }
const TSX: Lang = { name: 'TypeScript JSX', load: async () => (await import('@codemirror/lang-javascript')).javascript({ jsx: true, typescript: true }) }
const VUE: Lang = { name: 'Vue', load: async () => (await import('@codemirror/lang-vue')).vue() }
const JSON_: Lang = { name: 'JSON', load: async () => (await import('@codemirror/lang-json')).json() }
const CSS: Lang = { name: 'CSS', load: async () => (await import('@codemirror/lang-css')).css() }
const HTML: Lang = { name: 'HTML', load: async () => (await import('@codemirror/lang-html')).html() }
const MD: Lang = { name: 'Markdown', load: async () => (await import('@codemirror/lang-markdown')).markdown() }
const GO: Lang = { name: 'Go', load: async () => (await import('@codemirror/lang-go')).go() }
const YAML: Lang = { name: 'YAML', load: async () => (await import('@codemirror/lang-yaml')).yaml() }

const BY_EXT: Record<string, Lang> = {
  php: PHP, js: JS, mjs: JS, cjs: JS, ts: TS, mts: TS, cts: TS, jsx: JSX, tsx: TSX, vue: VUE,
  json: JSON_, css: CSS, scss: CSS, html: HTML, htm: HTML, md: MD, go: GO, yaml: YAML, yml: YAML,
}

export function langFor(name: string): Lang | null {
  const lower = name.toLowerCase()
  if (lower.endsWith('.blade.php')) return { ...PHP, name: 'Blade' }
  const ext = lower.includes('.') ? lower.slice(lower.lastIndexOf('.') + 1) : ''
  return BY_EXT[ext] ?? null
}
