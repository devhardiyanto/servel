// Language server untuk Kova Editor (spike 2). Satu klien per server per
// jendela; server dijalankan Rust (`editor::lsp`) dan dimulai saat file
// pertama yang ditanganinya dibuka. File Vue dilayani dua server, tetapi
// halaman ini hanya melihat satu (Rust yang menggabungkan).
import { reactive } from 'vue'
import { Channel, invoke } from '@tauri-apps/api/core'
import type { ChangeSet, Extension, Text } from '@codemirror/state'
import type { EditorView } from '@codemirror/view'
import {
  LSPClient, LSPPlugin, Workspace, languageServerExtensions, languageServerSupport,
  type Transport, type WorkspaceFile,
} from '@codemirror/lsp-client'
import { errorCode, errorText } from './api'

export type Server = 'php' | 'typescript' | 'vue' | 'go'

export const SERVER_LABEL: Record<Server, string> = {
  php: 'PHP: Phpactor', typescript: 'TypeScript: vtsls', vue: 'Vue: vue_ls + vtsls', go: 'Go: gopls',
}

/** Server dan languageId LSP untuk nama file, atau null bila tidak ada. */
export function serverFor(name: string): { server: Server; languageId: string } | null {
  const lower = name.toLowerCase()
  if (lower.endsWith('.blade.php')) return null
  const ext = lower.slice(lower.lastIndexOf('.') + 1)
  switch (ext) {
    case 'php': return { server: 'php', languageId: 'php' }
    case 'ts': case 'mts': case 'cts': return { server: 'typescript', languageId: 'typescript' }
    case 'tsx': return { server: 'typescript', languageId: 'typescriptreact' }
    case 'js': case 'mjs': case 'cjs': return { server: 'typescript', languageId: 'javascript' }
    case 'jsx': return { server: 'typescript', languageId: 'javascriptreact' }
    case 'vue': return { server: 'vue', languageId: 'vue' }
    case 'go': return { server: 'go', languageId: 'go' }
    default: return null
  }
}

/** URI file sama dengan yang dibuat Rust (`lsp::path_uri`). */
export function pathUri(path: string): string {
  let s = path.replace(/\\/g, '/')
  if (!s.startsWith('/')) s = '/' + s
  let out = 'file://'
  for (const ch of new TextEncoder().encode(s)) {
    const c = String.fromCharCode(ch)
    if (/[A-Za-z0-9\-_.~/]/.test(c)) out += c
    else if (c === ':' && out.length <= 10) out += c
    else out += '%' + ch.toString(16).toUpperCase().padStart(2, '0')
  }
  return out
}

/** Path dari URI, dengan pemisah yang sama dengan folder root. */
export function uriPath(uri: string, sep: string): string {
  let p = decodeURIComponent(uri.replace(/^file:\/\//, ''))
  if (/^\/[A-Za-z]:/.test(p)) p = p.slice(1)
  return sep === '\\' ? p.replace(/\//g, '\\') : p
}

/** Buang HTML berbahaya dari Markdown hover (server bisa saja disusupi). */
export function sanitize(html: string): string {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, 'text/html')
  doc.querySelectorAll('script,style,iframe,object,embed,link,meta,form,input,button,img').forEach((n) => n.remove())
  doc.querySelectorAll('*').forEach((el) => {
    for (const a of Array.from(el.attributes)) {
      const v = a.value.trim().toLowerCase()
      if (a.name.startsWith('on') || ((a.name === 'href' || a.name === 'src') && v.startsWith('javascript:'))) el.removeAttribute(a.name)
    }
  })
  return doc.body.firstElementChild?.innerHTML ?? ''
}

export interface ServerStatus { state: 'starting' | 'running' | 'missing' | 'stopped' | 'error'; text: string }
export const status = reactive<Partial<Record<Server, ServerStatus>>>({})

/** Workspace satu-view: berkas terbuka hanya selama tab-nya aktif di view. */
class KovaWorkspace extends Workspace {
  files: (WorkspaceFile & { view: EditorView })[] = []
  private versions: Record<string, number> = {}
  constructor(client: LSPClient, private display: (uri: string) => Promise<EditorView | null>) {
    super(client)
  }
  private nextVersion(uri: string): number {
    return (this.versions[uri] = (this.versions[uri] ?? -1) + 1)
  }
  syncFiles(): ReturnType<Workspace['syncFiles']> {
    const out: { changes: ChangeSet; file: WorkspaceFile; prevDoc: Text }[] = []
    for (const file of this.files) {
      const plugin = LSPPlugin.get(file.view)
      if (!plugin) continue
      const changes = plugin.unsyncedChanges
      if (!changes.empty) {
        out.push({ changes, file, prevDoc: file.doc })
        file.doc = file.view.state.doc
        file.version = this.nextVersion(file.uri)
        plugin.clear()
      }
    }
    return out
  }
  openFile(uri: string, languageId: string, view: EditorView): void {
    if (this.getFile(uri)) this.closeFile(uri)
    const file = { uri, languageId, version: this.nextVersion(uri), doc: view.state.doc, view, getView: () => view }
    this.files.push(file)
    this.client.didOpen(file)
  }
  closeFile(uri: string): void {
    const f = this.getFile(uri)
    if (!f) return
    this.files = this.files.filter((x) => x !== f)
    this.client.didClose(uri)
  }
  displayFile(uri: string): Promise<EditorView | null> {
    return this.display(uri)
  }
}

interface Entry { client: LSPClient; id: number | null }
const clients = new Map<Server, Entry>()
let rootUri = ''
let display: (uri: string) => Promise<EditorView | null> = async () => null
let notify: (text: string) => void = () => {}

export function setupLsp(root: string, opts: { display: typeof display; notify: typeof notify }): void {
  rootUri = pathUri(root)
  display = opts.display
  notify = opts.notify
}

function startServer(server: Server, entry: Entry): void {
  status[server] = { state: 'starting', text: `${SERVER_LABEL[server]} starting` }
  const handlers = new Set<(m: string) => void>()
  const channel = new Channel<string>()
  channel.onmessage = (m) => {
    if (m.includes('"kova/exit"')) {
      status[server] = { state: 'stopped', text: `${SERVER_LABEL[server]} stopped. Reopen the file to start it again.` }
      clients.delete(server)
      return
    }
    for (const h of handlers) h(m)
  }
  void invoke<number>('lsp_start', { server, onMessage: channel })
    .then((id) => {
      entry.id = id
      const transport: Transport = {
        send: (message) => { void invoke('lsp_send', { id, message }).catch(() => {}) },
        subscribe: (h) => handlers.add(h),
        unsubscribe: (h) => handlers.delete(h),
      }
      entry.client.connect(transport)
      void entry.client.initializing.then(
        () => { status[server] = { state: 'running', text: SERVER_LABEL[server] } },
        (e: unknown) => { status[server] = { state: 'error', text: `${SERVER_LABEL[server]}: ${String(e)}` } },
      )
    })
    .catch((e: unknown) => {
      const missing = errorCode(e) === 'missing'
      status[server] = { state: missing ? 'missing' : 'error', text: errorText(e) }
      notify(errorText(e))
    })
}

/** Ekstensi LSP untuk file ini (kosong bila tidak ada server untuknya). */
export function lspFor(path: string, name: string): { ext: Extension; server: Server | null } {
  const s = serverFor(name)
  if (!s || status[s.server]?.state === 'missing') return { ext: [], server: s?.server ?? null }
  let entry = clients.get(s.server)
  if (!entry) {
    const client = new LSPClient({
      rootUri,
      timeout: 10_000,
      sanitizeHTML: sanitize,
      workspace: (c) => new KovaWorkspace(c, display),
      extensions: languageServerExtensions(),
    })
    entry = { client, id: null }
    clients.set(s.server, entry)
    startServer(s.server, entry)
  }
  return { ext: languageServerSupport(entry.client, pathUri(path), s.languageId), server: s.server }
}
