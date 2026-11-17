<script setup lang="ts">
// Popup tray (v3.7.0, canvas "Tray & accessibility"): jendela kecil di dekat
// ikon tray. Dimuat di jendela terpisah (`index.html?view=tray`), jadi state
// dibaca sendiri: config, status project, dan status service dari event yang
// sama dengan jendela utama.
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getVersion } from '@tauri-apps/api/app'
import { openUrl } from '@tauri-apps/plugin-opener'
import KovaMark from '@/components/shell/KovaMark.vue'
import StatusDot, { type DotState } from '@/components/ui/StatusDot.vue'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime } from '@/composables/useProjectRuntime'
import type { Project } from '@/types/mise'
import type { ServiceDef, ServiceStatus } from '@/types/service'

const { config, load } = useConfig()
const { runtime, stateOf } = useProjectRuntime()

const version = ref('')
const defs = ref<ServiceDef[]>([])
const running = ref<Record<string, boolean>>({})
const busy = ref<Record<string, boolean>>({})
const error = ref('')
const root = ref<HTMLElement | null>(null)

const runningServices = computed(() => defs.value.filter((d) => running.value[d.id]).map((d) => d.name))
const selectedIds = computed(() => config.value.selectedServiceIds ?? [])

function runnable(p: Project): boolean {
  return !!p.path && !!p.command?.trim() && (p.port ?? 0) >= 1024
}

interface Row {
  project: Project
  dot: DotState
  hint: string
  /** Teks lengkap untuk pembaca layar. */
  label: string
  disabled: boolean
}

const rows = computed<Row[]>(() =>
  config.value.projects.map((p) => {
    const info = runtime.value[p.id]
    const state = stateOf(p.id)
    const name = p.name
    if (state === 'running') return { project: p, dot: 'running', hint: `:${p.port}`, label: `${name}, running. Open localhost:${p.port}`, disabled: false }
    if (state === 'unresponsive') return { project: p, dot: 'unresponsive', hint: `:${p.port}`, label: `${name}, not answering. Open localhost:${p.port}`, disabled: false }
    if (state === 'starting') return { project: p, dot: 'starting', hint: 'starting…', label: `${name}, starting`, disabled: true }
    if (!runnable(p)) return { project: p, dot: 'stopped', hint: 'not set up', label: `${name}, not set up yet`, disabled: true }
    if (state === 'exited') {
      const code = info?.exitCode !== null && info?.exitCode !== undefined ? ` (${info.exitCode})` : ''
      return { project: p, dot: 'exited', hint: `exited${code} · Start`, label: `${name}, exited${code}. Start`, disabled: false }
    }
    return { project: p, dot: 'stopped', hint: 'Start', label: `${name}, stopped. Start`, disabled: false }
  }),
)

async function hide(): Promise<void> {
  await invoke('tray_popup_hide').catch(() => {})
}

async function onProject(r: Row): Promise<void> {
  error.value = ''
  const p = r.project
  if (r.dot === 'running' || r.dot === 'unresponsive') {
    await openUrl(`http://localhost:${p.port}`).catch((e) => (error.value = String(e)))
    await hide()
    return
  }
  busy.value = { ...busy.value, [p.id]: true }
  try {
    await invoke('site_start', { project: p })
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = { ...busy.value, [p.id]: false }
  }
}

async function startServices(): Promise<void> {
  error.value = ''
  const ids = selectedIds.value
  if (!ids.length) return
  // Jendela utama menampilkan "starting" seperti dari menu tray.
  await emit('services-action', { action: 'start', services: ids })
  await invoke('services_start', { services: ids }).catch((e) => (error.value = String(e)))
}

async function stopServices(): Promise<void> {
  error.value = ''
  await emit('services-action', { action: 'stop_all', services: Object.keys(running.value).filter((id) => running.value[id]) })
  await invoke('services_stop_all').catch((e) => (error.value = String(e)))
}

async function refresh(): Promise<void> {
  error.value = ''
  try {
    await load()
  } catch {
    /* config tidak terbaca: daftar project kosong */
  }
  try {
    const list = await invoke<ServiceStatus[]>('services_status')
    running.value = Object.fromEntries(list.map((s) => [s.id, s.running]))
  } catch {
    /* engine tidak menjawab: anggap tidak ada yang berjalan */
  }
}

// Panah atas/bawah berpindah antar tombol; Esc menutup popup.
function onKey(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    e.preventDefault()
    void hide()
    return
  }
  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
  const items = Array.from(root.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])
  if (!items.length) return
  e.preventDefault()
  const i = items.indexOf(document.activeElement as HTMLButtonElement)
  const next = e.key === 'ArrowDown' ? (i + 1) % items.length : (i - 1 + items.length) % items.length
  items[next]?.focus()
}

// Fokus ke wadah, bukan item pertama: dibuka dengan mouse tidak memunculkan
// cincin fokus; panah bawah langsung memilih item pertama.
function focusFirst(): void {
  root.value?.focus()
}

// Laporkan tinggi isi supaya jendela pas dengan isinya (dan muncul pertama kali).
let observer: ResizeObserver | null = null
function report(): void {
  const h = root.value?.offsetHeight
  if (h) void invoke('tray_popup_resize', { height: h }).catch(() => {})
}

const unlisten: UnlistenFn[] = []
onMounted(async () => {
  version.value = await getVersion().catch(() => '')
  try {
    defs.value = await invoke<ServiceDef[]>('load_services')
  } catch {
    defs.value = []
  }
  await refresh()
  unlisten.push(
    await listen<Record<string, boolean>>('container-status-snapshot', (e) => (running.value = { ...e.payload })),
    await listen<{ service: string; running: boolean }>('container-status-changed', (e) => {
      running.value = { ...running.value, [e.payload.service]: e.payload.running }
    }),
    await listen('tray-popup-shown', async () => {
      await refresh()
      await nextTick()
      focusFirst()
    }),
  )
  await nextTick()
  if (root.value) {
    observer = new ResizeObserver(report)
    observer.observe(root.value)
  }
  report()
  focusFirst()
})
onUnmounted(() => {
  observer?.disconnect()
  for (const u of unlisten) u()
})
</script>

<template>
  <div ref="root" class="tp" role="dialog" aria-label="Kova" tabindex="-1" @keydown="onKey">
    <header class="tp-head">
      <KovaMark :size="18" />
      <span class="tp-name">Kova</span>
      <span class="tp-ver">{{ version }}</span>
    </header>

    <section v-if="rows.length" class="tp-sect" aria-labelledby="tp-projects">
      <h2 id="tp-projects" class="tp-label">Projects</h2>
      <button
        v-for="r in rows"
        :key="r.project.id"
        class="tp-item"
        :disabled="r.disabled || busy[r.project.id]"
        @click="onProject(r)"
      >
        <StatusDot :state="r.dot" />
        <span class="tp-text">{{ r.project.name }}<span class="sr-only">{{ r.label.slice(r.project.name.length) }}</span></span>
        <span class="tp-hint" :class="{ 'tp-hint--err': r.dot === 'exited' }" aria-hidden="true">{{ r.hint }}</span>
      </button>
    </section>

    <section class="tp-sect" aria-labelledby="tp-services">
      <h2 id="tp-services" class="tp-label">
        Services
        <span class="tp-count">{{ runningServices.length ? `${runningServices.length} running` : 'none running' }}</span>
      </h2>
      <p v-if="runningServices.length" class="tp-names">{{ runningServices.join(' · ') }}</p>
      <button class="tp-item" :disabled="!selectedIds.length" @click="startServices">
        <span class="tp-text">Start selected services</span>
        <span class="tp-hint" aria-hidden="true">{{ selectedIds.length }} selected</span>
      </button>
      <button v-if="runningServices.length" class="tp-item" @click="stopServices">
        <span class="tp-text">Stop all services</span>
      </button>
    </section>

    <p v-if="error" class="tp-error" role="alert">{{ error }}</p>

    <footer class="tp-sect tp-foot">
      <button class="tp-item" @click="invoke('app_open_main')"><span class="tp-text">Open Kova</span></button>
      <button class="tp-item" @click="invoke('app_quit')"><span class="tp-text">Quit Kova</span></button>
    </footer>
  </div>
</template>

<style scoped>
.tp:focus { outline: none; }
.tp {
  display: flex;
  flex-direction: column;
  padding: 6px;
  color: var(--text);
  font-size: var(--text-sm);
  user-select: none;
}
.tp-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 8px 10px;
}
.tp-name { font-weight: 600; }
.tp-ver { margin-left: auto; font: 400 var(--text-xs) var(--font-mono); color: var(--faint); }
.tp-sect {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 4px 0;
  border-top: 1px solid var(--border);
}
.tp-label {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  padding: 6px 10px 4px;
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--faint);
}
.tp-count { margin-left: auto; font-weight: 400; letter-spacing: 0; text-transform: none; color: var(--muted); }
.tp-names { margin: 0; padding: 0 10px 4px; font-size: var(--text-xs); color: var(--muted); }
.tp-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  min-height: 30px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.tp-item:hover:not(:disabled) { background: var(--surface3); }
.tp-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.tp-item:disabled { color: var(--muted); cursor: default; }
.tp-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tp-hint { flex: none; font: 400 var(--text-xs) var(--font-mono); color: var(--faint); }
.tp-hint--err { color: var(--red); }
.tp-error { margin: 0; padding: 6px 10px; font-size: var(--text-xs); color: var(--red); border-top: 1px solid var(--border); }
</style>

<style>
/* Jendela popup: latar panel, tanpa scrollbar (tinggi jendela = tinggi isi). */
html.tray-popup,
html.tray-popup body {
  background: var(--surface);
  overflow: hidden;
}
</style>
