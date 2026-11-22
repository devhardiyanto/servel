<script setup lang="ts">
// Popup tray: jendela kecil di dekat ikon tray. Dimuat di jendela terpisah
// (`index.html?view=tray`), jadi state dibaca sendiri: config, status project,
// dan status service dari event yang sama dengan jendela utama.
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getVersion } from '@tauri-apps/api/app'
import { openUrl } from '@tauri-apps/plugin-opener'
import KovaMark from '@/components/shell/KovaMark.vue'
import StatusDot, { type DotState } from '@/components/ui/StatusDot.vue'
import FrameworkLogo from '@/components/ui/FrameworkLogo.vue'
import ServiceLogo from '@/components/ui/ServiceLogo.vue'
import { groupIdOf, orderedProjects } from '@/utils/projectGroups'
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

// Versi global PHP/Node: pengganti submenu menu native.
type Tool = 'php' | 'node'
interface VersionRow { version: string; installed: boolean; global: boolean }
interface Overview { global: string | null; versions: VersionRow[] }
const TOOLS: { tool: Tool; label: string }[] = [
  { tool: 'php', label: 'PHP' },
  { tool: 'node', label: 'Node' },
]
const runtimes = ref<Partial<Record<Tool, Overview>>>({})
const expanded = ref<Tool | null>(null)
const switching = ref<string | null>(null)
const runtimeRows = computed(() =>
  TOOLS.filter((t) => runtimes.value[t.tool]).map((t) => {
    const o = runtimes.value[t.tool]!
    return { ...t, global: o.global, versions: o.versions.filter((v) => v.installed) }
  }),
)

async function loadRuntimes(): Promise<void> {
  const next: Partial<Record<Tool, Overview>> = {}
  await Promise.all(
    TOOLS.map(async ({ tool }) => {
      try {
        next[tool] = await invoke<Overview>('runtime_overview', { tool, projects: [] })
      } catch {
        /* mise tidak terpasang: bagian ini disembunyikan */
      }
    }),
  )
  runtimes.value = next
}

async function setGlobal(tool: Tool, version: string): Promise<void> {
  if (switching.value) return
  error.value = ''
  switching.value = `${tool}@${version}`
  try {
    await invoke('runtime_set_global', { tool, version })
    // Jendela utama (Runtimes, Settings) memuat ulang daftarnya.
    await emit('runtime-global-changed', { tool, version })
    await loadRuntimes()
    expanded.value = null
  } catch (e) {
    error.value = String(e)
  } finally {
    switching.value = null
  }
}

// Baris per service untuk yang dipilih atau sedang berjalan.
// Klik = start/stop service itu saja. `pending` menahan tampilan "starting…" /
// "stopping…" sampai event status datang (atau 90 detik).
const pending = ref<Record<string, 'start' | 'stop'>>({})
const pendingTimers: Record<string, ReturnType<typeof setTimeout>> = {}
function setPending(id: string, action: 'start' | 'stop' | null): void {
  clearTimeout(pendingTimers[id])
  const next = { ...pending.value }
  if (action) {
    next[id] = action
    pendingTimers[id] = setTimeout(() => setPending(id, null), 90_000)
  } else delete next[id]
  pending.value = next
}
interface SvcRow { def: ServiceDef; dot: DotState; hint: string; label: string }
const serviceRows = computed<SvcRow[]>(() =>
  defs.value
    .filter((d) => running.value[d.id] || selectedIds.value.includes(d.id) || pending.value[d.id])
    .map((d) => {
      const p = pending.value[d.id]
      if (p) return { def: d, dot: 'starting', hint: p === 'start' ? 'starting…' : 'stopping…', label: `${d.name}, ${p === 'start' ? 'starting' : 'stopping'}` }
      if (running.value[d.id]) return { def: d, dot: 'running', hint: 'Stop', label: `${d.name}, running. Stop` }
      return { def: d, dot: 'stopped', hint: 'Start', label: `${d.name}, stopped. Start` }
    }),
)

async function toggleService(r: SvcRow): Promise<void> {
  if (pending.value[r.def.id]) return
  error.value = ''
  const id = r.def.id
  if (running.value[id]) {
    setPending(id, 'stop')
    await emit('services-action', { action: 'stop', services: [id] })
    await invoke('services_stop', { services: [id] }).catch((e) => {
      error.value = String(e)
      setPending(id, null)
    })
  } else {
    setPending(id, 'start')
    await emit('services-action', { action: 'start', services: [id] })
    // Backend Docker menyusun compose dari daftar ini: sertakan yang sedang
    // berjalan supaya tidak ikut dihentikan, tanpa menyalakan pilihan lain.
    const keep = Object.keys(running.value).filter((k) => running.value[k])
    await invoke('services_start', { services: [...new Set([...keep, id])] }).catch((e) => {
      error.value = String(e)
      setPending(id, null)
    })
  }
}

const runningServices = computed(() => defs.value.filter((d) => running.value[d.id]).map((d) => d.name))
const selectedIds = computed(() => config.value.selectedServiceIds ?? [])

function runnable(p: Project): boolean {
  return !!p.path && !!p.command?.trim() && (p.port ?? 0) >= 1024
}

interface Row {
  project: Project
  dot: DotState
  hint: string
  /** Teks lengkap untuk pembaca layar: nama, status, kelompok, aksi. */
  label: string
  disabled: boolean
  /** Id kelompok yang ada; null = Ungrouped. */
  groupId: string | null
  groupName: string
}

const groups = computed(() => config.value.groups ?? [])

const allRows = computed<Row[]>(() =>
  orderedProjects(config.value.projects, groups.value).map((p) => {
    const info = runtime.value[p.id]
    const st = stateOf(p.id)
    const groupId = groupIdOf(p, groups.value)
    const groupName = groups.value.find((g) => g.id === groupId)?.name ?? ''
    const row = (dot: DotState, hint: string, state: string, action: string, disabled: boolean): Row => ({
      project: p,
      dot,
      hint,
      label: `${p.name}, ${state}${groupName ? `, ${groupName}` : ''}${action ? `. ${action}` : ''}`,
      disabled,
      groupId,
      groupName,
    })
    // Environment yang dipakai, bila bukan Default.
    const env = info?.envName ? ` · ${info.envName}` : ''
    if (st === 'running') return row('running', `:${p.port}${env}`, `running${env ? ` with ${info?.envName}` : ''}`, `Open localhost:${p.port}`, false)
    if (st === 'unresponsive') return row('unresponsive', `:${p.port}`, 'not answering', `Open localhost:${p.port}`, false)
    if (st === 'starting') return row('starting', 'starting…', 'starting', '', true)
    if (!runnable(p)) return row('stopped', 'not set up', 'not set up yet', '', true)
    if (st === 'exited') {
      const code = info?.exitCode !== null && info?.exitCode !== undefined ? ` (${info.exitCode})` : ''
      return row('exited', `exited${code} · Start`, `exited${code}`, 'Start', false)
    }
    return row('stopped', 'Start', 'stopped', 'Start', false)
  }),
)

// Daftar project tidak memanjangkan popup tanpa batas. Project aktif
// (berjalan, memulai, tidak menjawab, keluar dengan error) naik ke atas;
// sisanya mengikuti urutan kelompok dan project di Kova. Tertutup =
// PROJECT_LIMIT baris pertama; "Show all" membuka daftar lengkap per kelompok
// di kotak bergulir beserta kolom pencarian.
const PROJECT_LIMIT = 5
// Tinggi maksimal popup: MAX_HEIGHT di tray_popup.rs, atau tinggi work area
// monitor dikurangi margin bila layarnya lebih pendek.
const MAX_HEIGHT = 760
function computeMaxHeight(): number {
  const avail = window.screen?.availHeight || MAX_HEIGHT
  return Math.max(320, Math.min(MAX_HEIGHT, avail - 16))
}
const maxHeight = ref(computeMaxHeight())
const ACTIVE: DotState[] = ['running', 'unresponsive', 'starting', 'exited']
const showAll = ref(false)
const filter = ref('')
const sortedRows = computed(() => {
  const active = allRows.value.filter((r) => ACTIVE.includes(r.dot))
  return [...active, ...allRows.value.filter((r) => !ACTIVE.includes(r.dot))]
})
const activeCount = computed(() => allRows.value.filter((r) => r.dot === 'running' || r.dot === 'unresponsive').length)
const overflow = computed(() => sortedRows.value.length > PROJECT_LIMIT)
const showFilter = computed(() => showAll.value && overflow.value)
const grouped = computed(() => groups.value.length > 0)
const rows = computed<Row[]>(() => {
  if (!showAll.value) return sortedRows.value.slice(0, PROJECT_LIMIT)
  // Daftar lengkap dengan kelompok: urut per kelompok, bukan aktif dulu.
  const base = grouped.value ? allRows.value : sortedRows.value
  const q = filter.value.trim().toLowerCase()
  return q ? base.filter((r) => r.project.name.toLowerCase().includes(q) || r.groupName.toLowerCase().includes(q)) : base
})

interface Block { key: string; name: string; meta: string; rows: Row[] }
/** Daftar lengkap dipecah per kelompok (semua terbuka di tray); tanpa kelompok, satu blok tanpa judul. */
const blocks = computed<Block[]>(() => {
  if (!showAll.value || !grouped.value) return [{ key: '', name: '', meta: '', rows: rows.value }]
  const keys = [...groups.value.map((g) => g.id), null]
  return keys
    .map((id) => {
      const inGroup = allRows.value.filter((r) => r.groupId === id)
      const running = inGroup.filter((r) => r.dot === 'running' || r.dot === 'unresponsive').length
      const name = id ? (groups.value.find((g) => g.id === id)?.name ?? '') : 'Ungrouped'
      return {
        key: id ?? '~ungrouped',
        name,
        meta: `${inGroup.length}${running ? ` · ${running} running` : ''}`,
        rows: rows.value.filter((r) => r.groupId === id),
      }
    })
    .filter((b) => b.rows.length > 0)
})
const hiddenCount = computed(() => sortedRows.value.length - PROJECT_LIMIT)
function toggleShowAll(): void {
  showAll.value = !showAll.value
  filter.value = ''
}

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

// Selesaikan "starting…"/"stopping…" begitu keadaan service sesuai tujuan.
function settlePending(): void {
  for (const [id, a] of Object.entries(pending.value)) {
    if ((a === 'start') === !!running.value[id]) setPending(id, null)
  }
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
  // Tidak ditunggu: mise bisa butuh beberapa ratus ms, popup tampil lebih dulu.
  void loadRuntimes()
}

// Panah atas/bawah berpindah antar tombol; Esc menutup popup.
function onKey(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    e.preventDefault()
    // Esc pertama mengosongkan pencarian; berikutnya menutup popup.
    if (filter.value) filter.value = ''
    else void hide()
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
    await listen<Record<string, boolean>>('container-status-snapshot', (e) => {
      running.value = { ...e.payload }
      settlePending()
    }),
    await listen<{ service: string; running: boolean }>('container-status-changed', (e) => {
      running.value = { ...running.value, [e.payload.service]: e.payload.running }
      settlePending()
    }),
    await listen('tray-popup-shown', async () => {
      // Tiap kali dibuka mulai dari tampilan ringkas.
      showAll.value = false
      filter.value = ''
      expanded.value = null
      // Bisa dibuka di monitor lain dengan tinggi berbeda.
      maxHeight.value = computeMaxHeight()
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
  <div ref="root" class="tp" :style="{ maxHeight: `${maxHeight}px` }" role="dialog" aria-label="Kova" tabindex="-1" @keydown="onKey">
    <header class="tp-head">
      <KovaMark :size="18" />
      <span class="tp-name">Kova</span>
      <span class="tp-ver">{{ version }}</span>
    </header>

    <section v-if="allRows.length" class="tp-sect tp-sect--projects" aria-labelledby="tp-projects">
      <h2 id="tp-projects" class="tp-label">
        Projects
        <span class="tp-count">{{ activeCount ? `${activeCount} running · ` : '' }}{{ allRows.length }}</span>
      </h2>
      <label v-if="showFilter" class="tp-filter">
        <span class="sr-only">Find a project</span>
        <input v-model="filter" type="search" placeholder="Find a project" spellcheck="false" autocomplete="off" />
      </label>
      <div id="tp-project-list" class="tp-list" :class="{ 'tp-list--all': showAll, 'tp-list--keep': rows.length > 1 }">
        <template v-for="b in blocks" :key="b.key">
          <p v-if="b.name" class="tp-ghead"><span class="tp-ghead__n">{{ b.name }}</span> · {{ b.meta }}</p>
          <button
            v-for="r in b.rows"
            :key="r.project.id"
            class="tp-item"
            :disabled="r.disabled || busy[r.project.id]"
            @click="onProject(r)"
          >
            <StatusDot :state="r.dot" />
            <FrameworkLogo class="tp-logo" :kind="r.project.kind" :size="13" />
            <span class="tp-text">{{ r.project.name }}<span class="sr-only">{{ r.label.slice(r.project.name.length) }}</span></span>
            <span class="tp-hint" :class="{ 'tp-hint--err': r.dot === 'exited' }" aria-hidden="true">{{ r.hint }}</span>
          </button>
        </template>
        <p v-if="showAll && !rows.length" class="tp-empty">No project matches “{{ filter.trim() }}”.</p>
      </div>
      <button
        v-if="overflow"
        class="tp-item tp-more"
        :aria-expanded="showAll"
        aria-controls="tp-project-list"
        @click="toggleShowAll"
      >
        <span class="tp-text">{{ showAll ? 'Show fewer' : `Show all ${allRows.length} projects` }}</span>
        <span v-if="!showAll" class="tp-hint tp-hint--value">+{{ hiddenCount }}</span>
      </button>
    </section>

    <section class="tp-sect" aria-labelledby="tp-services">
      <h2 id="tp-services" class="tp-label">
        Services
        <span class="tp-count">{{ runningServices.length ? `${runningServices.length} running` : 'none running' }}</span>
      </h2>
      <button
        v-for="r in serviceRows"
        :key="r.def.id"
        class="tp-item"
        :disabled="!!pending[r.def.id]"
        @click="toggleService(r)"
      >
        <StatusDot :state="r.dot" />
        <ServiceLogo class="tp-logo" :id="r.def.id" :size="13" />
        <span class="tp-text">{{ r.def.name }}<span class="sr-only">{{ r.label.slice(r.def.name.length) }}</span></span>
        <span class="tp-hint" :class="{ 'tp-hint--value': pending[r.def.id] }" aria-hidden="true">{{ r.hint }}</span>
      </button>
      <!-- Pilihan kosong dijelaskan, bukan sekadar "0 selected". -->
      <button
        class="tp-item"
        :disabled="!selectedIds.length"
        :title="selectedIds.length ? undefined : 'Choose services in Kova → Services first'"
        @click="startServices"
      >
        <span class="tp-text">Start selected services</span>
        <span class="tp-hint" aria-hidden="true">{{ selectedIds.length ? `${selectedIds.length} selected` : 'none selected' }}</span>
      </button>
      <button v-if="runningServices.length" class="tp-item" @click="stopServices">
        <span class="tp-text">Stop all services</span>
      </button>
    </section>

    <section v-if="runtimeRows.length" class="tp-sect" aria-labelledby="tp-runtimes">
      <h2 id="tp-runtimes" class="tp-label">Global versions</h2>
      <template v-for="r in runtimeRows" :key="r.tool">
        <button
          class="tp-item"
          :aria-expanded="expanded === r.tool"
          :aria-controls="`tp-ver-${r.tool}`"
          :disabled="!r.versions.length"
          @click="expanded = expanded === r.tool ? null : r.tool"
        >
          <span class="tp-text">{{ r.label }}</span>
          <span class="tp-hint tp-hint--value">{{ r.global ?? (r.versions.length ? 'not set' : 'none installed') }}</span>
          <span class="tp-caret" :class="{ 'tp-caret--open': expanded === r.tool }" aria-hidden="true">›</span>
        </button>
        <div v-if="expanded === r.tool" :id="`tp-ver-${r.tool}`" class="tp-versions" role="group" :aria-label="`${r.label} versions`">
          <button
            v-for="v in r.versions"
            :key="v.version"
            class="tp-item tp-item--sub"
            :aria-pressed="v.global"
            :disabled="!!switching"
            @click="v.global ? (expanded = null) : setGlobal(r.tool, v.version)"
          >
            <span class="tp-check" aria-hidden="true">{{ v.global ? '✓' : '' }}</span>
            <span class="tp-text">{{ r.label }} {{ v.version }}</span>
            <span v-if="switching === `${r.tool}@${v.version}`" class="tp-hint tp-hint--value">switching…</span>
          </button>
        </div>
      </template>
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
  /* Batas tinggi dari `maxHeight` (≤ MAX_HEIGHT di tray_popup.rs). Lebih dari
     ini, hanya daftar project yang menyusut dan digulir. */
  box-sizing: border-box;
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
  flex: none;
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
/* Kontras petunjuk tetap cukup di atas latar hover. */
.tp-item:hover:not(:disabled) .tp-hint:not(.tp-hint--err) { color: var(--muted); }
.tp-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.tp-item:disabled { color: var(--muted); cursor: default; }
.tp-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tp-hint { flex: none; font: 400 var(--text-xs) var(--font-mono); color: var(--faint); }
.tp-hint--err { color: var(--red); }
/* Dibaca pembaca layar (tidak aria-hidden): kontras teks biasa. */
.tp-hint--value { color: var(--muted); }
.tp-caret { flex: none; width: 10px; color: var(--faint); transition: transform var(--dur-fast) var(--ease); }
.tp-caret--open { transform: rotate(90deg); }
.tp-versions { display: flex; flex-direction: column; gap: 1px; }
.tp-item--sub { padding-left: 18px; }
.tp-check { flex: none; width: 12px; color: var(--accent); }
@media (prefers-reduced-motion: reduce) { .tp-caret { transition: none; } }
.tp-sect--projects { flex: 0 1 auto; min-height: 0; }
.tp-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
}
/* Paling sedikit dua baris tetap terlihat saat popup penuh. */
.tp-list--keep { min-height: 62px; }
/* Daftar lengkap: kira-kira 8 baris, sisanya digulir di dalam kotak ini,
   jadi Services, versi global, Open Kova, dan Quit Kova tetap terlihat. */
.tp-list--all { max-height: 250px; }
.tp-more .tp-text { color: var(--muted); }
.tp-filter { display: block; padding: 2px 4px 4px; }
.tp-filter input {
  width: 100%;
  box-sizing: border-box;
  min-height: 28px;
  padding: 0 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--surface2);
  color: var(--text);
  font: inherit;
}
.tp-filter input:focus-visible { outline: 2px solid var(--accent); outline-offset: -1px; }
.tp-ghead {
  margin: 0;
  padding: 8px 10px 2px;
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--muted);
}
.tp-ghead__n { letter-spacing: 0.04em; text-transform: uppercase; }
.tp-ghead:first-child { padding-top: 2px; }
.tp-logo { margin-left: -2px; }
.tp-empty { margin: 0; padding: 6px 10px; font-size: var(--text-xs); color: var(--muted); }
.tp-error { margin: 0; padding: 6px 10px; font-size: var(--text-xs); color: var(--red); border-top: 1px solid var(--border); }
</style>

<style>
/* Jendela popup: latar panel. Tinggi jendela = tinggi isi, sampai batas
   tinggi jendela (MAX_HEIGHT di tray_popup.rs); lebih dari itu isi digulir. */
html.tray-popup,
html.tray-popup body {
  background: var(--surface);
}
html.tray-popup #app {
  overflow-x: hidden;
  overflow-y: auto;
}
</style>
