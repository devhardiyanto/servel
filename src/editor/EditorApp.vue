<script setup lang="ts">
// Kova Editor (spike v4.0): satu jendela per folder. Explorer, tab dengan
// preview, CodeMirror 6, simpan dengan deteksi konflik, Quick Open (Ctrl P),
// palet (Ctrl Shift P). LSP dan terminal menyusul di spike 2 dan 3.
import { computed, markRaw, nextTick, onBeforeUnmount, onMounted, reactive, ref, shallowRef, type Ref } from 'vue'
import { Compartment, EditorState, type Extension, type Text } from '@codemirror/state'
import {
  EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, highlightSpecialChars,
  drawSelection, dropCursor, rectangularSelection, crosshairCursor,
} from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { bracketMatching, foldGutter, foldKeymap, indentOnInput } from '@codemirror/language'
import { searchKeymap, highlightSelectionMatches, gotoLine, openSearchPanel } from '@codemirror/search'
import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from '@codemirror/autocomplete'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  editorRoot, listDir, indexFiles, readFile, writeFile, statFile, showMain, errorCode, errorText, type DirEntry,
} from './api'
import { langFor } from './languages'
import { kovaTheme, kovaHighlight } from './theme'
import { rank } from './fuzzy'
import { chord } from './keys'

type Eol = '\n' | '\r\n'
interface Banner { kind: 'conflict' | 'changed' | 'deleted' | 'error' | 'large'; text: string }
interface Tab {
  path: string
  name: string
  rel: string
  state: EditorState
  saved: Text
  mtime: number | null
  eol: Eol
  bom: boolean
  large: boolean
  readonly: boolean
  lang: string
  preview: boolean
  dirty: boolean
  banner: Banner | null
}

const win = getCurrentWindow()
const root = ref('')
const rootName = computed(() => root.value.split(/[\\/]/).filter(Boolean).pop() ?? root.value)
const sep = computed(() => (root.value.includes('\\') ? '\\' : '/'))
const loadError = ref('')

// ---------- Explorer
const children = reactive(new Map<string, DirEntry[]>())
const expanded = reactive(new Set<string>())
interface Row { e: DirEntry; depth: number }
const rows = computed<Row[]>(() => {
  const out: Row[] = []
  const walk = (dir: string, depth: number) => {
    for (const e of children.get(dir) ?? []) {
      out.push({ e, depth })
      if (e.dir && expanded.has(e.path)) walk(e.path, depth + 1)
    }
  }
  walk(root.value, 0)
  return out
})
const focusedRow = ref(0)
const treeEl = ref<HTMLElement | null>(null)

async function loadDir(path: string): Promise<void> {
  try {
    children.set(path, await listDir(path))
  } catch (e) {
    toast(`${path}: ${errorText(e)}`)
  }
}
async function toggleDir(e: DirEntry): Promise<void> {
  if (expanded.has(e.path)) {
    expanded.delete(e.path)
    return
  }
  if (!children.has(e.path)) await loadDir(e.path)
  expanded.add(e.path)
}
function relOf(path: string): string {
  const r = root.value
  return path.startsWith(r) ? path.slice(r.length).replace(/^[\\/]+/, '').replace(/\\/g, '/') : path
}
function clickRow(i: number, e: DirEntry, pin: boolean): void {
  focusedRow.value = i
  if (e.dir) void toggleDir(e)
  else void openFile(e.path, { preview: !pin })
}
function treeKey(ev: KeyboardEvent): void {
  const list = rows.value
  if (!list.length) return
  const cur = list[focusedRow.value]
  let handled = true
  if (ev.key === 'ArrowDown') focusedRow.value = Math.min(list.length - 1, focusedRow.value + 1)
  else if (ev.key === 'ArrowUp') focusedRow.value = Math.max(0, focusedRow.value - 1)
  else if (ev.key === 'ArrowRight' && cur?.e.dir && !expanded.has(cur.e.path)) void toggleDir(cur.e)
  else if (ev.key === 'ArrowLeft' && cur?.e.dir && expanded.has(cur.e.path)) expanded.delete(cur.e.path)
  else if (ev.key === 'Enter' && cur) clickRow(focusedRow.value, cur.e, true)
  else if (ev.key === ' ' && cur && !cur.e.dir) void openFile(cur.e.path, { preview: true })
  else handled = false
  if (handled) {
    ev.preventDefault()
    void nextTick(() => treeEl.value?.querySelector<HTMLElement>(`[data-row="${focusedRow.value}"]`)?.focus())
  }
}

// ---------- Editor
// state dan saved diberi markRaw: Vue tidak membungkus objek CodeMirror.
const tabs = ref([]) as Ref<Tab[]>
const active = ref(-1)
const current = computed<Tab | null>(() => tabs.value[active.value] ?? null)
const host = ref<HTMLElement | null>(null)
const view = shallowRef<EditorView | null>(null)
const cursor = reactive({ line: 1, col: 1, sel: 0 })
const langComp = new Compartment()
const wrapComp = new Compartment()
const wrap = ref(false)

function onUpdate(u: { docChanged: boolean; selectionSet: boolean; state: EditorState }): void {
  const t = current.value
  if (!t) return
  if (u.docChanged) {
    t.dirty = !u.state.doc.eq(t.saved)
    if (t.preview) t.preview = false
  }
  if (u.docChanged || u.selectionSet) {
    const r = u.state.selection.main
    const line = u.state.doc.lineAt(r.head)
    cursor.line = line.number
    cursor.col = r.head - line.from + 1
    cursor.sel = Math.abs(r.to - r.from)
  }
}

function extensions(lang: Extension, readonly: boolean): Extension[] {
  return [
    lineNumbers(), highlightActiveLineGutter(), highlightSpecialChars(), history(), foldGutter(), drawSelection(),
    dropCursor(), EditorState.allowMultipleSelections.of(true), indentOnInput(), bracketMatching(), closeBrackets(),
    autocompletion(), rectangularSelection(), crosshairCursor(), highlightActiveLine(), highlightSelectionMatches(),
    keymap.of([
      // Pintasan VS Code yang berbeda dari default CodeMirror.
      { key: 'Mod-g', run: gotoLine },
      { key: 'Mod-h', run: openSearchPanel },
      ...closeBracketsKeymap, ...defaultKeymap, ...searchKeymap, ...historyKeymap, ...foldKeymap, ...completionKeymap,
      indentWithTab,
    ]),
    kovaTheme, kovaHighlight, langComp.of(lang), wrapComp.of(wrap.value ? EditorView.lineWrapping : []),
    EditorState.readOnly.of(readonly),
    EditorView.contentAttributes.of({ 'aria-label': 'Editor' }),
    EditorView.updateListener.of(onUpdate),
  ]
}

function stash(): void {
  const t = current.value
  if (t && view.value) t.state = markRaw(view.value.state)
}
function activate(i: number): void {
  if (i === active.value && view.value?.state === tabs.value[i]?.state) return
  stash()
  active.value = i
  const t = tabs.value[i]
  if (!t || !view.value) return
  view.value.setState(t.state)
  view.value.dispatch({ effects: wrapComp.reconfigure(wrap.value ? EditorView.lineWrapping : []) })
  onUpdate({ docChanged: false, selectionSet: true, state: t.state })
  view.value.focus()
}

// Klik lalu dobel klik memanggil openFile dua kali sebelum file selesai dibaca.
const opening = new Map<string, Promise<void>>()
async function openFile(path: string, opts: { preview?: boolean; line?: number } = {}): Promise<void> {
  const pending = opening.get(path)
  if (pending) {
    await pending
    return openFile(path, opts)
  }
  const p = openFileNow(path, opts)
  opening.set(path, p)
  try { await p } finally { opening.delete(path) }
}

async function openFileNow(path: string, opts: { preview?: boolean; line?: number }): Promise<void> {
  const preview = opts.preview ?? false
  const found = tabs.value.findIndex((t) => t.path === path)
  if (found >= 0) {
    if (!preview) tabs.value[found].preview = false
    activate(found)
    if (opts.line) goLine(opts.line)
    return
  }
  let ft
  try {
    ft = await readFile(path)
  } catch (e) {
    toast(`${path.split(/[\\/]/).pop()}: ${errorText(e)}`)
    return
  }
  const name = path.split(/[\\/]/).pop() ?? path
  const lang = ft.large ? null : langFor(name)
  const langExt = lang ? await lang.load() : []
  const state = EditorState.create({ doc: ft.text, extensions: extensions(langExt, ft.readonly) })
  const tab: Tab = {
    path, name, rel: relOf(path), state: markRaw(state), saved: markRaw(state.doc), mtime: ft.mtime,
    eol: ft.eol, bom: ft.bom, large: ft.large, readonly: ft.readonly, lang: lang?.name ?? 'Plain text',
    preview, dirty: false,
    banner: ft.large ? { kind: 'large', text: `${name} is ${Math.round(ft.size / 1024 / 1024)} MB. Opened as plain text without highlighting, so the window stays fast.` } : null,
  }
  stash()
  // Satu tab preview: file preview baru menggantikannya.
  const pv = preview ? tabs.value.findIndex((t) => t.preview && !t.dirty) : -1
  if (pv >= 0) {
    tabs.value.splice(pv, 1, tab)
    active.value = -1
    activate(pv)
  } else {
    const at = active.value >= 0 ? active.value + 1 : tabs.value.length
    tabs.value.splice(at, 0, tab)
    active.value = -1
    activate(at)
  }
  if (opts.line) goLine(opts.line)
}

function goLine(n: number): void {
  const v = view.value
  if (!v) return
  const line = v.state.doc.line(Math.max(1, Math.min(n, v.state.doc.lines)))
  v.dispatch({ selection: { anchor: line.from }, scrollIntoView: true })
  v.focus()
}

function docOf(t: Tab): EditorState {
  return t === current.value && view.value ? view.value.state : t.state
}

async function save(t: Tab | null = current.value, force = false): Promise<boolean> {
  if (!t) return false
  const st = docOf(t)
  try {
    t.mtime = await writeFile(t.path, st.doc.toString(), t.eol, t.bom, force ? null : t.mtime)
    t.saved = markRaw(st.doc)
    t.dirty = false
    t.preview = false
    if (t.banner?.kind !== 'large') t.banner = null
    return true
  } catch (e) {
    const code = errorCode(e)
    t.banner = code === 'conflict'
      ? { kind: 'conflict', text: `${t.name} changed on disk after you started editing. Nothing was saved yet.` }
      : { kind: 'error', text: `Couldn't save ${t.name}: ${errorText(e)}` }
    return false
  }
}
async function saveAll(): Promise<void> {
  for (const t of tabs.value) if (t.dirty) await save(t)
}

/** Ganti isi tab dengan versi di disk (bisa di-undo dengan Ctrl Z). */
async function useDisk(t: Tab): Promise<void> {
  let ft
  try {
    ft = await readFile(t.path)
  } catch (e) {
    t.banner = { kind: 'error', text: errorText(e) }
    return
  }
  const st = docOf(t)
  const tr = st.update({ changes: { from: 0, to: st.doc.length, insert: ft.text } })
  if (t === current.value && view.value) view.value.dispatch(tr)
  else t.state = markRaw(tr.state)
  t.saved = markRaw(tr.state.doc)
  t.mtime = ft.mtime
  t.eol = ft.eol
  t.bom = ft.bom
  t.dirty = false
  t.banner = null
}

/** Saat jendela mendapat fokus: file yang berubah atau hilang di disk. */
async function checkDisk(): Promise<void> {
  for (const t of tabs.value) {
    let m: number | null
    try { m = await statFile(t.path) } catch { continue }
    if (m === null) {
      t.banner = { kind: 'deleted', text: `${t.name} was deleted on disk. Your copy is still open. Save to put it back.` }
      t.mtime = null
    } else if (t.mtime !== null && m !== t.mtime) {
      if (!t.dirty) {
        await useDisk(t)
        toast(`${t.name} reloaded: it changed on disk and had no unsaved edits.`)
      } else {
        t.banner = { kind: 'changed', text: `${t.name} changed on disk. Saving will ask which version to keep.` }
      }
    }
  }
}

// ---------- tutup tab dan jendela
interface Ask { title: string; body: string; files: string[]; resolve: (r: 'save' | 'discard' | 'cancel') => void }
const asking = ref<Ask | null>(null)
function askSave(title: string, body: string, files: string[]): Promise<'save' | 'discard' | 'cancel'> {
  return new Promise((resolve) => { asking.value = { title, body, files, resolve } })
}
function answer(r: 'save' | 'discard' | 'cancel'): void {
  asking.value?.resolve(r)
  asking.value = null
  if (r === 'cancel') view.value?.focus()
}

async function closeTab(i = active.value): Promise<void> {
  const t = tabs.value[i]
  if (!t) return
  if (t.dirty) {
    const r = await askSave(`Save changes to ${t.name}?`, "Don't save discards the changes in this file.", [])
    if (r === 'cancel') return
    if (r === 'save' && !(await save(t))) return
  }
  if (i === active.value) stash()
  tabs.value.splice(i, 1)
  if (!tabs.value.length) {
    active.value = -1
    view.value?.setState(EditorState.create({ doc: '', extensions: [kovaTheme, EditorState.readOnly.of(true)] }))
    return
  }
  const next = Math.min(i, tabs.value.length - 1)
  active.value = -1
  activate(next)
}

// ---------- Quick Open dan palet
const qo = reactive({ open: false, text: '', sel: 0 })
const qoInput = ref<HTMLInputElement | null>(null)
const index = shallowRef<string[] | null>(null)
const indexing = ref(false)
interface Cmd { label: string; keys?: string; run: () => void | Promise<void> }
const commands: Cmd[] = [
  { label: 'File: Save', keys: 'Ctrl S', run: () => void save() },
  { label: 'File: Save All', keys: 'Ctrl K S', run: saveAll },
  { label: 'View: Close Editor', keys: 'Ctrl W', run: () => closeTab() },
  { label: 'Go to Line…', keys: 'Ctrl G', run: () => { qo.open = true; qo.text = ':' } },
  { label: 'View: Toggle Word Wrap', keys: 'Alt Z', run: toggleWrap },
  { label: 'Kova: Show main window', run: () => void showMain() },
  { label: 'Explorer: Focus', keys: 'Ctrl Shift E', run: focusTree },
]
interface Item { label: string; detail?: string; keys?: string; run: () => void | Promise<void> }
const items = computed<Item[]>(() => {
  const t = qo.text
  if (t.startsWith('>')) {
    const q = t.slice(1).trim().toLowerCase()
    return commands.filter((c) => c.label.toLowerCase().includes(q))
  }
  if (t.startsWith(':')) {
    const n = parseInt(t.slice(1), 10)
    return [{ label: Number.isFinite(n) ? `Go to line ${n}` : 'Type a line number', run: () => { if (Number.isFinite(n)) goLine(n) } }]
  }
  const list = index.value ?? []
  if (!t.trim()) {
    // Tanpa query: tab terbuka dulu (paling baru di depan), lalu file lain.
    return tabs.value.map((x) => ({ label: x.name, detail: x.rel, run: () => openFile(x.path) }))
  }
  return rank(t, list).map((m) => {
    const name = m.path.split('/').pop() ?? m.path
    return { label: name, detail: m.path, run: () => openFile(root.value + sep.value + m.path.replace(/\//g, sep.value)) }
  })
})
async function openQuick(prefix = ''): Promise<void> {
  qo.open = true
  qo.text = prefix
  qo.sel = 0
  await nextTick()
  qoInput.value?.focus()
  if (!index.value && !indexing.value) {
    indexing.value = true
    try { index.value = markRaw(await indexFiles()) } catch (e) { toast(errorText(e)) } finally { indexing.value = false }
  }
}
function closeQuick(): void {
  qo.open = false
  view.value?.focus()
}
function qoKey(e: KeyboardEvent): void {
  if (e.key === 'ArrowDown') { qo.sel = Math.min(items.value.length - 1, qo.sel + 1); e.preventDefault() }
  else if (e.key === 'ArrowUp') { qo.sel = Math.max(0, qo.sel - 1); e.preventDefault() }
  else if (e.key === 'Enter') { const it = items.value[qo.sel]; qo.open = false; e.preventDefault(); if (it) void it.run() }
  else if (e.key === 'Escape') { e.preventDefault(); closeQuick() }
}

function toggleWrap(): void {
  wrap.value = !wrap.value
  view.value?.dispatch({ effects: wrapComp.reconfigure(wrap.value ? EditorView.lineWrapping : []) })
}
function focusTree(): void {
  void nextTick(() => treeEl.value?.querySelector<HTMLElement>(`[data-row="${focusedRow.value}"]`)?.focus())
}

// ---------- pintasan jendela (fase bubble, setelah CodeMirror)
let chordK = 0
function onKey(e: KeyboardEvent): void {
  const c = chord(e)
  if (chordK && Date.now() - chordK < 1500) {
    chordK = 0
    if (c === 'S' || c === 'Ctrl+S') { e.preventDefault(); void saveAll(); return }
  }
  const map: Record<string, () => void> = {
    'Ctrl+S': () => void save(),
    'Ctrl+P': () => void openQuick(),
    'Ctrl+Shift+P': () => void openQuick('>'),
    'F1': () => void openQuick('>'),
    'Ctrl+W': () => void closeTab(),
    'Ctrl+F4': () => void closeTab(),
    'Alt+Z': toggleWrap,
    'Ctrl+Shift+E': focusTree,
    'Ctrl+Tab': () => tabs.value.length && activate((active.value + 1) % tabs.value.length),
    'Ctrl+Shift+Tab': () => tabs.value.length && activate((active.value - 1 + tabs.value.length) % tabs.value.length),
    'Ctrl+K': () => { chordK = Date.now() },
  }
  const fn = map[c]
  if (fn) {
    e.preventDefault()
    fn()
  }
}

// ---------- toast
const toastText = ref('')
let toastTimer = 0
function toast(text: string): void {
  toastText.value = text
  window.clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => (toastText.value = ''), 5000)
}

/** Fokus awal dialog pada tombol default (Save). */
const vFocus = { mounted: (el: HTMLElement) => el.focus() }

const dirtyCount = computed(() => tabs.value.filter((t) => t.dirty).length)

let unFocus: (() => void) | null = null
let unClose: (() => void) | null = null
onMounted(async () => {
  view.value = markRaw(new EditorView({ parent: host.value!, state: EditorState.create({ doc: '', extensions: [kovaTheme, EditorState.readOnly.of(true)] }) }))
  window.addEventListener('keydown', onKey)
  try {
    root.value = await editorRoot()
  } catch (e) {
    loadError.value = errorText(e)
    return
  }
  document.title = `${rootName.value} · Kova Editor`
  await loadDir(root.value)
  unFocus = await win.onFocusChanged(({ payload }) => { if (payload) void checkDisk() })
  // Spike: tanya sebelum menutup jendela dengan file belum disimpan. Hot exit
  // (F9 di canvas) menggantikan pertanyaan ini di MVP.
  unClose = await win.onCloseRequested(async (ev) => {
    const dirty = tabs.value.filter((t) => t.dirty)
    if (!dirty.length) return
    ev.preventDefault()
    const r = await askSave(`Save changes to ${dirty.length} file${dirty.length > 1 ? 's' : ''}?`, 'Closing this window.', dirty.map((t) => t.rel))
    if (r === 'cancel') return
    if (r === 'save') {
      for (const t of dirty) if (!(await save(t))) return
    }
    for (const t of tabs.value) t.dirty = false
    await win.destroy()
  })
  focusTree()
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  unFocus?.()
  unClose?.()
  view.value?.destroy()
})
</script>

<template>
  <div class="ke">
    <aside class="ke-side" aria-label="Explorer">
      <div class="ke-sh"><span>Explorer</span><span v-if="dirtyCount" class="ke-badge" :aria-label="`${dirtyCount} unsaved`">{{ dirtyCount }}</span></div>
      <div class="ke-root" :title="root">{{ rootName }}</div>
      <div ref="treeEl" class="ke-tree" role="tree" :aria-label="`Files in ${rootName}`" @keydown="treeKey">
        <button
          v-for="(r, i) in rows" :key="r.e.path" :data-row="i" type="button" role="treeitem"
          class="ke-row" :class="{ sel: current?.path === r.e.path }" :style="{ paddingLeft: `${10 + r.depth * 14}px` }"
          :tabindex="i === focusedRow ? 0 : -1" :aria-expanded="r.e.dir ? expanded.has(r.e.path) : undefined"
          :title="r.e.name" @click="clickRow(i, r.e, false)" @dblclick="clickRow(i, r.e, true)"
        >
          <span class="ke-caret" aria-hidden="true">{{ r.e.dir ? (expanded.has(r.e.path) ? '▾' : '▸') : '' }}</span>
          <span class="ke-name">{{ r.e.name }}</span>
        </button>
        <p v-if="loadError" class="ke-err">{{ loadError }}</p>
      </div>
    </aside>

    <main class="ke-main">
      <div class="ke-tabs" role="tablist">
        <div
          v-for="(t, i) in tabs" :key="t.path" class="ke-tab" :class="{ on: i === active, pv: t.preview }" role="tab"
          :aria-selected="i === active" :title="t.rel" tabindex="0"
          @click="activate(i)" @dblclick="t.preview = false" @keydown.enter="activate(i)" @auxclick.prevent="closeTab(i)"
        >
          <span class="nm" :class="{ del: t.banner?.kind === 'deleted' }">{{ t.name }}</span>
          <span v-if="t.dirty" class="dot" aria-label="unsaved">●</span>
          <button v-else type="button" class="x" :aria-label="`Close ${t.name}`" @click.stop="closeTab(i)">×</button>
        </div>
      </div>
      <div v-if="current" class="ke-crumb">{{ current.rel.split('/').join(' › ') }}</div>
      <div v-if="current?.banner" class="ke-banner" :class="current.banner.kind" role="status">
        <span class="bt">{{ current.banner.text }}</span>
        <template v-if="current.banner.kind === 'conflict' || current.banner.kind === 'changed'">
          <button type="button" class="ke-btn" @click="save(current, true)">Overwrite with mine</button>
          <button type="button" class="ke-btn" @click="useDisk(current)">Use disk version</button>
        </template>
        <button v-else-if="current.banner.kind === 'deleted'" type="button" class="ke-btn" @click="save(current, true)">Save to restore</button>
        <button v-else-if="current.banner.kind === 'error'" type="button" class="ke-btn" @click="save(current)">Try again</button>
      </div>
      <div ref="host" class="ke-host" :class="{ empty: !current }"></div>
      <div v-if="!current" class="ke-empty">
        <p><b>{{ rootName }}</b></p>
        <p>Ctrl P opens a file · Ctrl Shift P runs a command</p>
      </div>
    </main>

    <footer class="ke-status">
      <span>{{ rootName }}</span>
      <span v-if="indexing">Indexing files…</span>
      <span class="sp"></span>
      <template v-if="current">
        <span v-if="current.readonly">Read-only</span>
        <span>Ln {{ cursor.line }}, Col {{ cursor.col }}<template v-if="cursor.sel"> ({{ cursor.sel }} selected)</template></span>
        <span>{{ current.eol === '\r\n' ? 'CRLF' : 'LF' }}</span>
        <span>{{ current.bom ? 'UTF-8 with BOM' : 'UTF-8' }}</span>
        <span>{{ current.lang }}</span>
      </template>
      <span class="spike">Spike</span>
    </footer>

    <div v-if="qo.open" class="ke-scrim" @mousedown.self="closeQuick">
      <div class="ke-qo" role="dialog" aria-label="Quick Open">
        <input
          ref="qoInput" v-model="qo.text" class="ke-qi" spellcheck="false" aria-label="Search files"
          :placeholder="indexing ? 'Indexing files…' : 'Search files by name. > for commands, : for a line'"
          @keydown="qoKey" @input="qo.sel = 0"
        />
        <ul class="ke-ql" role="listbox">
          <li
            v-for="(it, i) in items" :key="it.label + (it.detail ?? '') + i" role="option" :aria-selected="i === qo.sel"
            :class="{ on: i === qo.sel }" @mousedown.prevent="qo.open = false; it.run()" @mousemove="qo.sel = i"
          >
            <span class="l">{{ it.label }}</span><span v-if="it.detail" class="d">{{ it.detail }}</span><span v-if="it.keys" class="k">{{ it.keys }}</span>
          </li>
          <li v-if="!items.length" class="none">{{ indexing ? 'Indexing files…' : `No files match "${qo.text}". Try fewer letters, or type > for commands.` }}</li>
        </ul>
      </div>
    </div>

    <div v-if="asking" class="ke-scrim">
      <div class="ke-modal" role="dialog" aria-modal="true" :aria-label="asking.title" @keydown.esc="answer('cancel')">
        <h2>{{ asking.title }}</h2>
        <ul v-if="asking.files.length" class="ke-files"><li v-for="f in asking.files" :key="f">{{ f }}</li></ul>
        <p>{{ asking.body }}</p>
        <div class="acts">
          <button type="button" class="ke-btn" @click="answer('cancel')">Cancel</button>
          <button type="button" class="ke-btn" @click="answer('discard')">Don't save</button>
          <button v-focus type="button" class="ke-btn pri" @click="answer('save')">Save</button>
        </div>
      </div>
    </div>

    <div v-if="toastText" class="ke-toast" role="status">{{ toastText }}</div>
  </div>
</template>

<style>
html.kova-editor, html.kova-editor body, html.kova-editor #app { height: 100%; margin: 0; overflow: hidden; background: var(--bg); }
</style>

<style scoped>
.ke { height: 100%; display: grid; grid-template-columns: 260px minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) 24px; color: var(--text); font: 400 13px/1.4 var(--font-sans); background: var(--bg); }
.ke-side { grid-row: 1; display: flex; flex-direction: column; min-height: 0; background: var(--surface); border-right: 1px solid var(--border); }
.ke-sh { height: 34px; display: flex; align-items: center; gap: 8px; padding: 0 14px; font-size: 12px; font-weight: 600; color: var(--muted); }
.ke-badge { min-width: 16px; height: 16px; padding: 0 4px; border-radius: 8px; background: var(--accent); color: var(--bg); font-size: 11px; display: inline-flex; align-items: center; justify-content: center; }
.ke-root { padding: 2px 14px 6px; font-weight: 600; font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.ke-tree { flex: 1; min-height: 0; overflow: auto; padding-bottom: 12px; }
.ke-row { display: flex; align-items: center; gap: 4px; width: 100%; height: 22px; border: 0; background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; padding-right: 8px; white-space: nowrap; }
.ke-row:hover { background: var(--surface2); }
.ke-row.sel { background: rgba(111, 195, 234, .12); }
.ke-row:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.ke-caret { width: 12px; flex: none; color: var(--muted); font-size: 10px; }
.ke-name { overflow: hidden; text-overflow: ellipsis; }
.ke-err { padding: 8px 14px; color: var(--red); }
.ke-main { grid-row: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; position: relative; }
.ke-tabs { height: 34px; flex: none; display: flex; overflow-x: auto; background: var(--surface); border-bottom: 1px solid var(--border); scrollbar-width: thin; }
.ke-tab { display: flex; align-items: center; gap: 8px; padding: 0 8px 0 12px; flex: none; white-space: nowrap; color: var(--muted); border-right: 1px solid var(--border); cursor: pointer; }
.ke-tab.on { background: var(--bg); color: var(--text); box-shadow: inset 0 2px 0 var(--accent); }
.ke-tab:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.ke-tab.pv .nm { font-style: italic; }
.ke-tab .nm.del { text-decoration: line-through; color: var(--red); }
.ke-tab .dot { color: var(--amber); font-size: 10px; width: 18px; text-align: center; }
.ke-tab .x { width: 18px; height: 18px; border: 0; border-radius: 4px; background: none; color: var(--faint); cursor: pointer; font-size: 14px; line-height: 1; }
.ke-tab .x:hover, .ke-tab .x:focus-visible { background: var(--surface3); color: var(--text); }
.ke-crumb { height: 24px; flex: none; display: flex; align-items: center; padding: 0 14px; font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; }
.ke-banner { display: flex; align-items: center; gap: 8px; margin: 6px 12px; padding: 8px 12px; border-radius: 8px; border: 1px solid var(--border-strong); background: var(--surface2); font-size: 13px; }
.ke-banner .bt { flex: 1; }
.ke-banner.conflict, .ke-banner.changed { border-color: rgba(240, 198, 116, .45); }
.ke-banner.deleted, .ke-banner.error { border-color: rgba(241, 154, 154, .45); }
.ke-host { flex: 1; min-height: 0; }
.ke-host.empty { flex: 0; height: 0; overflow: hidden; }
.ke-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; color: var(--muted); }
.ke-empty p { margin: 4px 0; }
.ke-status { grid-column: 1 / 3; display: flex; align-items: center; gap: 14px; padding: 0 10px; border-top: 1px solid var(--border); background: var(--surface); font-size: 12px; color: var(--muted); white-space: nowrap; }
.ke-status .sp { flex: 1; }
.ke-status .spike { color: var(--amber); }
.ke-btn { height: 26px; padding: 0 10px; border-radius: 6px; border: 1px solid var(--border-strong); background: var(--surface2); color: var(--text); font: 500 12px var(--font-sans); cursor: pointer; white-space: nowrap; }
.ke-btn:hover { background: var(--surface3); }
.ke-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.ke-btn.pri { background: var(--accent); border-color: var(--accent); color: var(--bg); }
.ke-scrim { position: fixed; inset: 0; z-index: 50; background: rgba(0, 0, 0, .35); display: flex; justify-content: center; align-items: flex-start; }
.ke-qo { margin-top: 40px; width: min(560px, calc(100% - 32px)); background: var(--surface2); border: 1px solid var(--border-strong); border-radius: 8px; box-shadow: var(--shadow-md); padding: 6px; }
.ke-qi { width: 100%; box-sizing: border-box; height: 30px; padding: 0 10px; border-radius: 6px; border: 1px solid var(--accent); background: var(--bg); color: var(--text); font: 13px var(--font-sans); outline: none; }
.ke-ql { list-style: none; margin: 6px 0 0; padding: 0; max-height: 400px; overflow: auto; }
.ke-ql li { display: flex; align-items: center; gap: 10px; height: 28px; padding: 0 10px; border-radius: 4px; cursor: pointer; white-space: nowrap; }
.ke-ql li.on { background: var(--surface3); }
.ke-ql .l { flex: none; }
.ke-ql .d { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--muted); font-size: 12px; }
.ke-ql .k { margin-left: auto; color: var(--muted); font-size: 12px; }
.ke-ql li.none { color: var(--muted); cursor: default; white-space: normal; height: auto; padding: 8px 10px; }
.ke-modal { margin-top: 18vh; width: min(440px, calc(100% - 32px)); background: var(--surface); border: 1px solid var(--border-strong); border-radius: 12px; box-shadow: var(--shadow-md); padding: 18px 20px; }
.ke-modal h2 { margin: 0 0 8px; font-size: 16px; }
.ke-modal p { margin: 0; color: var(--muted); }
.ke-files { margin: 0 0 10px; padding: 0 0 0 18px; color: var(--text); font-size: 12px; }
.ke-modal .acts { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
.ke-toast { position: fixed; right: 16px; bottom: 36px; z-index: 60; max-width: 420px; padding: 10px 12px; border-radius: 8px; border: 1px solid var(--border-strong); background: var(--surface2); box-shadow: var(--shadow-md); }
</style>
