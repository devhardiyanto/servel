<script setup lang="ts">
// Panel detail project: kepala dengan status dan aksi, lalu tab Overview,
// Environment, Logs.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import PortOwnerBanner from '@/components/PortOwnerBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiMenu from '@/components/ui/UiMenu.vue'
import { tabPanelAttrs } from '@/utils/tabs'
import UiTabs from '@/components/ui/UiTabs.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiModal from '@/components/ui/UiModal.vue'
import StatusDot from '@/components/ui/StatusDot.vue'
import UiIcon from '@/components/ui/UiIcon.vue'
import UiChip from '@/components/ui/UiChip.vue'
import FrameworkChip from '@/components/FrameworkChip.vue'
import UiStatusNotice from '@/components/ui/UiStatusNotice.vue'
import { parseDotenv, serializeDotenv, mergeRows, summarize, type SkippedLine } from '@/utils/dotenv'
import LogTail from '@/components/LogTail.vue'
import ProcessesPanel from '@/components/ProcessesPanel.vue'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import { useServices } from '@/composables/useServices'
import { useEngineStatus } from '@/composables/useEngineStatus'
import { useToast } from '@/composables/useToast'
import { useUnsavedGuard, type UnsavedSource } from '@/composables/useUnsavedGuard'
import { useProjectRuntime, isActive } from '@/composables/useProjectRuntime'
import { useProjectActions, canRun, missingReason } from '@/composables/useProjectActions'
import type { Project, ToolCurrent } from '@/types/mise'
import { wslDistro, wslLinuxPath } from '@/utils/wsl'

const props = defineProps<{ project: Project }>()

const { config, updateProject } = useConfig()
const { lines } = useLogs()
const services = useServices()
const toast = useToast()
const runtime = useProjectRuntime()
const actions = useProjectActions()
const tab = actions.detailTab

const p = computed(() => props.project)
const state = computed(() => runtime.stateOf(p.value.id))
const info = computed(() => runtime.infoOf(p.value.id))
const busy = computed(() => !!actions.busy.value[p.value.id])
const error = computed(() => actions.errors.value[p.value.id] ?? '')
/** Start ditolak karena port dipakai program lain: port itu. */
const busyPort = computed(() => {
  const m = /^port (\d+) is used by another program/.exec(error.value)
  return m ? Number(m[1]) : null
})
const portSuggestion = ref<number | null>(null)
watch(busyPort, async (port) => {
  portSuggestion.value = null
  if (!port) return
  try {
    const r = await invoke<{ suggestion: number | null }>('project_port_check', { port, taken: actions.takenPorts(p.value.id) })
    portSuggestion.value = r.suggestion
  } catch {
    portSuggestion.value = null
  }
}, { immediate: true })
/** Port yang dicetak app, bila berbeda dari port project. */
const printedPort = computed(() => {
  const n = info.value?.printedPort ?? null
  return n && n !== p.value.port ? n : null
})
async function usePort(port: number | null): Promise<void> {
  if (!port) return
  actions.setError(p.value.id, '')
  await updateProject(p.value.id, { port })
  const updated = config.value.projects.find((x) => x.id === p.value.id)
  if (updated) await (isActive(state.value) ? actions.restart(updated) : actions.start(updated))
}
async function openPrinted(): Promise<void> {
  if (printedPort.value) await openUrl(`http://localhost:${printedPort.value}/`).catch(() => {})
}
const up = computed(() => state.value === 'running' || state.value === 'unresponsive')

// Mode Dockerfile (canvas K2/K4): skrip run mencetak "$ <engine> build …" lalu
// "$ <engine> run …". Langkah terakhir yang tercetak menentukan fasenya.
const lastStep = computed(() => {
  for (let i = projectLines.value.length - 1; i >= 0; i--) {
    if (projectLines.value[i].text.startsWith('$ ')) return i
  }
  return -1
})
const inBuild = computed(
  () => p.value.runMode === 'dockerfile' && lastStep.value >= 0 && /^\$ \S+ build /.test(projectLines.value[lastStep.value].text),
)
/** "step 3 of 4" dari baris build `[3/4]` (Docker BuildKit dan WSLC sama). */
const buildStep = computed(() => {
  if (!inBuild.value) return ''
  for (let i = projectLines.value.length - 1; i > lastStep.value; i--) {
    const m = /\[(\d+)\/(\d+)\]/.exec(projectLines.value[i].text)
    if (m) return `step ${m[1]} of ${m[2]}`
  }
  return ''
})
const buildFailed = computed(() => state.value === 'exited' && inBuild.value)

const statusLine = computed(
  () =>
    (state.value === 'starting' && inBuild.value ? `building image${buildStep.value ? ` · ${buildStep.value}` : ''}` : actions.stateLabel(p.value)) +
    (state.value === 'running' ? ` · :${p.value.port}` : '') +
    (isActive(state.value) && info.value?.envName ? ` · ${info.value.envName}` : ''),
)
/** Distro WSL tempat command dijalankan; `null` = Windows/OS ini. */
const runsInWsl = computed(() => (p.value.wsl && (p.value.runMode ?? 'command') === 'command' ? wslDistro(p.value.path) : null))
/** Run with: Dockerfile / Compose → "Container · kova/<nama>". */
const containerLabel = computed(() => {
  const base = p.value.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 40) || 'project'
  // Sama dengan instance_slug di sites/container.rs: slug + 8 karakter id.
  const short = p.value.id.replace(/[^a-zA-Z0-9]/g, '').slice(0, 8).toLowerCase()
  const slug = short ? `${base}-${short}` : base
  if (p.value.runMode === 'dockerfile') return `Container · kova/${slug} (${p.value.container?.dockerfile || 'Dockerfile'})`
  if (p.value.runMode === 'compose') return `Compose · ${p.value.container?.composeFile || 'compose.yaml'} (project kova-${slug})`
  return ''
})
/** Canvas K7: container project compose dan statusnya, diperbarui selama berjalan. */
const composeContainers = ref<{ service: string; state: string; ports: number[] }[]>([])
let composeTimer: ReturnType<typeof setInterval> | undefined
async function loadCompose(): Promise<void> {
  if (p.value.runMode !== 'compose' || !isActive(state.value)) {
    composeContainers.value = []
    return
  }
  composeContainers.value = await invoke<{ service: string; state: string; ports: number[] }[]>('project_compose_ps', { projectId: p.value.id }).catch(() => [])
}
watch(
  () => [p.value.runMode, state.value] as const,
  () => {
    clearInterval(composeTimer)
    void loadCompose()
    if (p.value.runMode === 'compose' && isActive(state.value)) composeTimer = setInterval(() => void loadCompose(), 5000)
  },
  { immediate: true },
)
onBeforeUnmount(() => clearInterval(composeTimer))

/** Canvas K3: bagaimana container menjangkau service Kova. */
const { engine } = useEngineStatus()
const containerNetwork = computed(() =>
  engine.value === 'wslc'
    ? 'Ports on this PC (WSL containers has no shared network). Environment values are passed with -e.'
    : p.value.container?.joinServices === false
      ? 'Own network; services through their ports on this PC. Environment values are passed with -e.'
      : 'servel_default · services by name, e.g. servel_mysql:3306. Environment values are passed with -e.',
)
const tools = ref<ToolCurrent[] | null>(null)
async function loadTools(): Promise<void> {
  if (!p.value.path) {
    tools.value = []
    return
  }
  try {
    tools.value = runsInWsl.value
      ? await invoke<ToolCurrent[]>('wsl_project_tools', { path: p.value.path })
      : await invoke<ToolCurrent[]>('mise_project_tools', { path: p.value.path })
  } catch {
    tools.value = []
  }
}
onMounted(loadTools)
watch(() => [p.value.path, p.value.wsl], loadTools)

const profile = computed(() =>
  p.value.profileId ? config.value.profiles.find((x) => x.id === p.value.profileId) : undefined,
)
const profileServices = computed(() =>
  (profile.value?.serviceIds ?? []).map((id) => ({
    id,
    name: services.definitions.value.find((d) => d.id === id)?.name ?? id,
    state: services.uiState.value[id]?.status === 'running' ? ('running' as const) : ('stopped' as const),
  })),
)

const projectLines = computed(() => lines.value.filter((l) => l.src === p.value.name))
const recent = computed(() => projectLines.value.slice(-12))
const last500 = computed(() => projectLines.value.slice(-500))

const TOOL_LABEL: Record<string, string> = { php: 'PHP', node: 'Node', go: 'Go', python: 'Python' }
function toolLabel(t: ToolCurrent): string {
  return TOOL_LABEL[t.tool] ?? t.tool
}

function sourceLabel(t: ToolCurrent): string {
  if (t.scope === 'global') return 'global'
  return t.source?.split(/[\\/]/).pop() ?? ''
}

async function openIn(target: 'folder' | 'terminal' | 'vscode'): Promise<void> {
  try {
    await invoke('project_open_in', { target, path: p.value.path, projectId: p.value.id })
  } catch (err) {
    actions.setError(p.value.id, String(err))
  }
}
// Default + environment lain. Environment lain hanya menyimpan nilai yang
// berbeda; baris Default tampil dengan nilai Default sebagai placeholder, dan
// baru disimpan bila diisi.
interface Row { k: string; v: string; ph?: string; inh?: boolean }
const rows = ref<Row[]>([])
const envError = ref('')
const envSaving = ref(false)

// "Edit as .env" dan paste banyak baris sekaligus.
const textMode = ref(false)
const envText = ref('')
const pasteNote = ref<{ title: string; skipped: SkippedLine[] } | null>(null)

const envs = computed(() => p.value.envs ?? [])
/** Environment yang dipakai saat start ('' = Default). Nama usang = Default. */
const activeName = computed(() => {
  const n = p.value.activeEnv ?? ''
  return envs.value.some((e) => e.name === n) ? n : ''
})
/** "Default + 2 changes": an environment only stores what differs from Default. */
const activeChanges = computed(() => {
  const n = Object.keys(envs.value.find((e) => e.name === activeName.value)?.vars ?? {}).length
  return `Default + ${n} ${n === 1 ? 'change' : 'changes'}`
})
const envOptions = computed(() => [
  { value: '', label: 'Default' },
  ...envs.value.map((e) => ({ value: e.name, label: e.name })),
])
/** Environment proses yang sedang jalan ('' = Default), dari status backend. */
const runningEnv = computed(() => info.value?.envName ?? '')
const restartToApply = computed(() => isActive(state.value) && runningEnv.value !== activeName.value)

function savedMap(name: string): Record<string, string> {
  if (!name) return { ...(p.value.env ?? {}) }
  return { ...(envs.value.find((e) => e.name === name)?.vars ?? {}) }
}

function buildRows(name: string, overrides: Record<string, string>): Row[] {
  if (!name) return Object.entries(overrides).map(([k, v]) => ({ k, v }))
  const defaults = p.value.env ?? {}
  const inherited: Row[] = Object.entries(defaults).map(([k, dv]) => ({ k, v: overrides[k] ?? '', ph: dv, inh: true }))
  const added: Row[] = Object.entries(overrides).filter(([k]) => !(k in defaults)).map(([k, v]) => ({ k, v }))
  // What this environment changes goes first: overrides of Default, then its
  // own variables, then the Default rows it leaves alone. Sorted only when the
  // rows are built, so a row does not jump while the user types in it.
  return [...inherited.filter((r) => r.v !== ''), ...added, ...inherited.filter((r) => r.v === '')]
}

// Search shows once the list is long enough to need it.
const ENV_SEARCH_MIN = 10
const envSearch = ref('')
const envHelpOpen = ref(false)
const showEnvSearch = computed(() => !textMode.value && rows.value.length > ENV_SEARCH_MIN)
/** Rows with their index in `rows`. New rows without a name always show. */
const visibleRows = computed(() => {
  const q = showEnvSearch.value ? envSearch.value.trim().toLowerCase() : ''
  const all = rows.value.map((r, i) => ({ r, i }))
  if (!q) return all
  return all.filter(({ r }) => r.k.trim() === '' || r.k.toLowerCase().includes(q) || r.v.toLowerCase().includes(q))
})

/**
 * Di environment bernama, baris dibagi dua kelompok: yang diatur environment
 * ini (override + variabel sendiri) lalu baris Default yang tidak diubah.
 * Batasnya dihitung saat baris dibangun, supaya baris tidak pindah kelompok
 * saat diketik.
 */
const defaultStart = ref(0)
function setRows(next: Row[]): void {
  rows.value = next
  const at = next.findIndex((r) => r.inh && r.v === '')
  defaultStart.value = at === -1 ? next.length : at
}
const showGroups = computed(() => !!activeName.value && !textMode.value && !(showEnvSearch.value && envSearch.value.trim()))
async function addRow(): Promise<void> {
  let at = rows.value.length
  if (activeName.value) {
    at = defaultStart.value
    rows.value.splice(at, 0, { k: '', v: '' })
    defaultStart.value++
  } else {
    rows.value.push({ k: '', v: '' })
  }
  // Baris baru bisa berada jauh di atas tombol: bawa fokus ke sana.
  await nextTick()
  const el = document.querySelector<HTMLInputElement>(`#pd-panel [aria-label="Name of variable ${at + 1}"]`)
  el?.focus()
  el?.scrollIntoView({ block: 'center' })
}

/** Baris yang benar-benar disimpan: di environment lain, baris Default kosong = tidak di-override. */
function effectiveRows(src: Row[]): Row[] {
  return src.filter((r) => !(r.inh && r.v === '') && (r.k.trim() !== '' || r.v !== ''))
}

function resetRows(): void {
  setRows(buildRows(activeName.value, savedMap(activeName.value)))
  if (textMode.value) envText.value = serializeDotenv(effectiveRows(rows.value))
  envError.value = ''
}
watch(
  () => [p.value.id, activeName.value],
  () => {
    textMode.value = false
    pasteNote.value = null
    envSearch.value = ''
    resetRows()
  },
  { immediate: true },
)

function noteFor(title: string, skipped: SkippedLine[]): void {
  pasteNote.value = { title, skipped }
}
const pasteDetail = computed(() =>
  pasteNote.value?.skipped.map((s) => `Line ${s.line}: ${s.text} (${s.reason})`).join('\n') || undefined,
)

function rowsFromText(): Row[] {
  const r = parseDotenv(envText.value)
  if (r.skipped.length) noteFor(`${r.entries.length} variables · ${r.skipped.length} skipped`, r.skipped)
  return buildRows(activeName.value, Object.fromEntries(r.entries.map((e) => [e.k, e.v])))
}

function toText(): void {
  envText.value = serializeDotenv(effectiveRows(rows.value))
  textMode.value = true
  pasteNote.value = null
}
function toRows(): void {
  pasteNote.value = null
  setRows(rowsFromText())
  textMode.value = false
}

/**
 * Paste ke kolom nama (atau nilai, bila teksnya banyak baris): teks
 * `KEY=VALUE` dipecah menjadi baris. Nama yang sudah ada diganti nilainya.
 */
function onPaste(e: ClipboardEvent, i: number, field: 'k' | 'v'): void {
  const t = e.clipboardData?.getData('text') ?? ''
  const multi = /\r?\n/.test(t.trim())
  if (field === 'v' ? !multi : !multi && !t.includes('=')) return
  const r = parseDotenv(t)
  if (r.entries.length === 0 && r.skipped.length === 0) return
  e.preventDefault()
  const m = mergeRows(rows.value, r.entries, i)
  setRows(m.rows.length ? (m.rows as Row[]) : [{ k: '', v: '' }])
  noteFor(summarize(m.added, m.replaced, r.skipped.length), r.skipped)
}

function removeRow(i: number): void {
  const r = rows.value[i]
  if (r.inh) r.v = '' // kembali ke nilai Default
  else {
    rows.value.splice(i, 1)
    if (i < defaultStart.value) defaultStart.value--
  }
}

function mapOf(src: Row[]): Record<string, string> {
  return Object.fromEntries(effectiveRows(src).map((r) => [r.k.trim(), r.v]))
}
function sameMap(a: Record<string, string>, b: Record<string, string>): boolean {
  const ka = Object.keys(a)
  return ka.length === Object.keys(b).length && ka.every((k) => b[k] === a[k])
}
const envDirty = computed(() => {
  const src = textMode.value
    ? buildRows(activeName.value, Object.fromEntries(parseDotenv(envText.value).entries.map((e) => [e.k, e.v])))
    : rows.value
  return !sameMap(mapOf(src), savedMap(activeName.value))
})

function validateEnv(): Record<string, string> | null {
  const env: Record<string, string> = {}
  for (const [i, r] of rows.value.entries()) {
    const k = r.k.trim()
    if (r.inh && r.v === '') continue
    if (k === '' && r.v === '') continue
    if (!/^[^\s=]+$/.test(k)) {
      envError.value = `Row ${i + 1}: "${k || '(empty)'}" is not a valid variable name.`
      return null
    }
    if (k.toUpperCase() === 'PORT') {
      envError.value = `Row ${i + 1}: set the port in Project settings, not here.`
      return null
    }
    if (k in env) {
      envError.value = `Row ${i + 1}: ${k} is listed twice.`
      return null
    }
    env[k] = r.v
  }
  envError.value = ''
  return env
}

async function saveEnv(thenRestart: boolean): Promise<boolean> {
  if (textMode.value) setRows(rowsFromText())
  const vars = validateEnv()
  if (!vars) {
    tab.value = 'env'
    return false
  }
  envSaving.value = true
  try {
    const name = activeName.value
    if (!name) await updateProject(p.value.id, { env: vars })
    else await updateProject(p.value.id, { envs: envs.value.map((e) => (e.name === name ? { name, vars } : e)) })
    resetRows()
    const fresh = config.value.projects.find((x) => x.id === p.value.id)
    if (thenRestart && fresh) {
      await (isActive(state.value) ? actions.restart(fresh) : actions.start(fresh))
      tab.value = 'overview'
    } else {
      toast.show(`${name || 'Default'} saved. It applies the next time the project starts.`)
    }
    return true
  } catch (err) {
    envError.value = `Could not save: ${String(err)}`
    tab.value = 'env'
    return false
  } finally {
    envSaving.value = false
  }
}

// Perubahan yang belum disimpan ikut dijaga saat pindah project, menutup
// panel, pindah layar, atau ganti environment.
const guard = useUnsavedGuard()
const guardSource: UnsavedSource = {
  get label() {
    return activeName.value || 'Default'
  },
  get owner() {
    return p.value.name
  },
  save: () => saveEnv(false),
  discard: () => {
    textMode.value = false
    pasteNote.value = null
    resetRows()
  },
}
watch(envDirty, (dirty) => guard.setSource(dirty ? guardSource : null), { immediate: true })
onBeforeUnmount(() => {
  if (guard.source.value === guardSource) guard.setSource(null)
})

async function selectEnv(name: string): Promise<void> {
  if (name === activeName.value) return
  if (!(await guard.confirmLeave())) return
  await updateProject(p.value.id, { activeEnv: name || null })
}

const ENV_NAME = /^[A-Za-z0-9_-]{1,40}$/
const newEnvOpen = ref(false)
const newEnvName = ref('')
const newEnvCopy = ref('')
const newEnvError = computed(() => {
  const n = newEnvName.value.trim()
  if (!n) return ''
  if (!ENV_NAME.test(n)) return 'Use letters, numbers, - and _ (up to 40).'
  if (n.toLowerCase() === 'default' || envs.value.some((e) => e.name.toLowerCase() === n.toLowerCase())) return `${n} already exists.`
  return ''
})
function openNewEnv(): void {
  newEnvName.value = ''
  newEnvCopy.value = ''
  newEnvOpen.value = true
}
async function createEnv(): Promise<void> {
  const name = newEnvName.value.trim()
  if (!name || newEnvError.value) return
  const vars = newEnvCopy.value ? savedMap(newEnvCopy.value) : {}
  await updateProject(p.value.id, { envs: [...envs.value, { name, vars }], activeEnv: name })
  newEnvOpen.value = false
  toast.show(`Created ${name}. Default's variables still apply; set only what changes.`)
}

const deleteEnvOpen = ref(false)
async function deleteEnv(): Promise<void> {
  const name = activeName.value
  if (!name) return
  await updateProject(p.value.id, { envs: envs.value.filter((e) => e.name !== name), activeEnv: null })
  deleteEnvOpen.value = false
  toast.show(`Deleted ${name}. ${p.value.name} uses Default now.`)
}

const defaultCount = computed(() => Object.keys(p.value.env ?? {}).length)
/** Canvas I10: Terminal here membawa Environment aktif dan versi mise project. */
const terminalTitle = computed(
  () => `Terminal here, with ${activeName.value || 'Default'} from the Environment tab and this project's mise versions`,
)

const tabs = computed(() => [
  { value: 'overview', label: 'Overview' },
  { value: 'env', label: 'Environment', badge: Object.keys(p.value.env ?? {}).length || undefined },
  { value: 'logs', label: 'Logs' },
])
</script>

<template>
  <div class="pd">
    <header class="pd-head">
      <div class="pd-titlerow">
        <div class="pd-title">
          <h2 class="pd-name">{{ p.name }}</h2>
          <StatusDot :state="state" :label="statusLine" />
        </div>
        <UiMenu :items="actions.menuItems(p)" :label="`More actions for ${p.name}`" @select="actions.onMenu(p, $event)" />
      </div>
      <div class="pd-actions">
        <template v-if="!p.path">
          <UiButton variant="accent" @click="actions.editingId.value = p.id">Choose folder</UiButton>
        </template>
        <template v-else-if="!canRun(p)">
          <UiButton variant="accent" @click="actions.editingId.value = p.id">Project settings</UiButton>
        </template>
        <template v-else-if="up">
          <UiButton variant="accent" @click="actions.open(p)"><UiIcon name="external" />Open localhost:{{ p.port }}</UiButton>
          <UiButton :disabled="busy" @click="actions.stop(p)"><UiIcon name="stop" />Stop</UiButton>
          <UiButton :disabled="busy" @click="actions.restart(p)"><UiIcon name="restart" />Restart</UiButton>
        </template>
        <template v-else-if="state === 'starting'">
          <UiButton disabled loading>Starting…</UiButton>
          <UiButton @click="actions.stop(p)"><UiIcon name="stop" />Stop</UiButton>
        </template>
        <UiButton v-else variant="success" :loading="busy" @click="actions.start(p)"><UiIcon v-if="!busy" name="play" />Start</UiButton>

      </div>
    </header>

    <UiTabs v-model="tab" :tabs="tabs" label="Project details" id-base="pd" />

    <div v-if="tab === 'overview'" class="pd-body" v-bind="tabPanelAttrs('pd', tab)">
      <PortOwnerBanner
        v-if="busyPort"
        :key="busyPort"
        :port="busyPort"
        :subject="p.name"
        :suggestion="portSuggestion"
        @use-port="usePort"
        @stopped="actions.start(p)"
      />
      <UiBanner v-else-if="error" tone="err" :title="`Something went wrong with ${p.name}`">{{ error }}</UiBanner>
      <UiBanner v-else-if="buildFailed" tone="err" title="The image did not build">
        <span v-if="buildStep">{{ buildStep[0].toUpperCase() + buildStep.slice(1) }}: </span><code v-if="info?.lastLine" class="pd-lastline">{{ info.lastLine }}</code>
        <template #actions>
          <UiButton @click="actions.viewLog(p)">View log</UiButton>
          <UiButton @click="openIn('vscode')">Open in VS Code</UiButton>
          <UiButton variant="accent" :loading="busy" @click="actions.start(p)">Retry</UiButton>
        </template>
      </UiBanner>
      <UiBanner
        v-else-if="state === 'exited'"
        tone="err"
        :title="info?.exitCode != null ? `Stopped with exit code ${info.exitCode}` : 'Stopped'"
      >
        <code v-if="info?.lastLine" class="pd-lastline">{{ info.lastLine }}</code>
        <template #actions>
          <UiButton @click="actions.viewLog(p)">View log</UiButton>
          <UiButton @click="tab = 'env'">Environment</UiButton>
          <UiButton variant="accent" :loading="busy" @click="actions.start(p)">Start again</UiButton>
        </template>
      </UiBanner>
      <UiBanner v-else-if="state === 'unresponsive' && printedPort" tone="warn" :title="`Nothing answers on :${p.port}`">
        The app printed <code class="pd-mono">http://localhost:{{ printedPort }}/</code>. Its command probably ignores the port Kova sets.
        <template #actions>
          <UiButton variant="accent" :loading="busy" @click="usePort(printedPort)">Use {{ printedPort }} for {{ p.name }}</UiButton>
          <UiButton @click="openPrinted"><UiIcon name="external" />Open localhost:{{ printedPort }}</UiButton>
          <UiButton @click="actions.viewLog(p)">View log</UiButton>
        </template>
      </UiBanner>
      <UiBanner v-else-if="state === 'unresponsive'" tone="warn" :title="`Running, but nothing answers on :${p.port}`">
        Still checking; this changes to Running as soon as the app answers. If it listens on another port, add {port} to the command.
        <template #actions>
          <UiButton @click="actions.editingId.value = p.id">Edit command</UiButton>
          <UiButton @click="actions.viewLog(p)">View log</UiButton>
        </template>
      </UiBanner>
      <p v-if="!canRun(p)" class="pd-note">{{ missingReason(p) }}</p>

      <dl class="pd-grid">
        <dt>Type</dt>
        <dd><FrameworkChip v-if="p.kind" :kind="p.kind" /><template v-else>Not detected</template></dd>
        <dt>Command</dt>
        <dd class="pd-mono pd-wrap">{{ containerLabel ? (p.runMode === 'compose' ? 'docker compose up --build' : 'build + run') : p.command || 'No command set' }}</dd>
        <dt>Port</dt>
        <dd class="pd-mono">{{ (p.port ?? 0) > 0 ? `localhost:${p.port}` : 'Not set' }}</dd>
        <dt>Folder</dt>
        <dd class="pd-mono pd-wrap pd-muted">{{ p.path || 'Not chosen yet' }}</dd>
        <template v-if="containerLabel">
          <dt>Runs in</dt>
          <dd>{{ containerLabel }}</dd>
          <dt>Network</dt>
          <dd class="pd-dim">{{ containerNetwork }}</dd>
        </template>
        <template v-if="composeContainers.length">
          <dt>Containers</dt>
          <dd>
            <ul class="pd-containers">
              <li v-for="c in composeContainers" :key="c.service">
                <span class="pd-mono">{{ c.service }}</span>
                <span v-if="c.ports.length" class="pd-mono pd-dim">:{{ c.ports.join(', :') }}</span>
                <span :class="c.state === 'running' ? 'pd-ok' : 'pd-dim'">{{ c.state }}</span>
              </li>
            </ul>
          </dd>
        </template>
        <template v-else-if="runsInWsl">
          <dt>Runs in</dt>
          <dd>WSL · {{ runsInWsl }} <span class="pd-mono pd-muted">{{ wslLinuxPath(p.path) }}</span></dd>
        </template>
        <template v-if="!envs.length">
          <dt>Environment</dt>
          <dd class="pd-dim">{{ defaultCount ? `Default · ${defaultCount} ${defaultCount === 1 ? 'variable' : 'variables'}` : 'No variables' }}</dd>
        </template>
        <template v-else>
          <dt>Environment</dt>
          <dd class="pd-rt">
            <UiChip tone="accent">{{ activeName || 'Default' }}</UiChip>
            <span v-if="activeName && !restartToApply" class="pd-dim">{{ activeChanges }}</span>
            <span v-if="restartToApply" class="pd-dim">running with {{ runningEnv || 'Default' }} · restart to apply</span>
          </dd>
        </template>
        <template v-if="tools === null">
          <dt>Runtime</dt>
          <dd class="pd-dim">Checking…</dd>
        </template>
        <template v-else-if="tools.length === 0">
          <dt>Runtime</dt>
          <dd class="pd-dim">No versions set by mise for this folder.</dd>
        </template>
        <template v-else>
          <template v-for="t in tools" :key="t.tool">
            <dt>{{ toolLabel(t) }}</dt>
            <dd class="pd-rt">
              <span class="pd-mono">{{ t.version }}</span>
              <UiChip v-if="sourceLabel(t)">{{ sourceLabel(t) }}</UiChip>
              <UiChip v-if="!t.installed" tone="warn">not installed</UiChip>
            </dd>
          </template>
        </template>
      </dl>

      <ProcessesPanel v-if="!p.runMode || p.runMode === 'command'" :project="p" />

      <section class="pd-sec">
        <h3 class="pd-label">Services{{ profile ? ` · profile ${profile.name}` : '' }}</h3>
        <p v-if="!profile" class="pd-dim">No service profile. Pick one in Project settings.</p>
        <p v-else-if="profileServices.length === 0" class="pd-dim">This profile has no services.</p>
        <ul v-else class="pd-chips">
          <li v-for="s in profileServices" :key="s.id">
            <UiChip><StatusDot :state="s.state" />{{ s.name }}</UiChip>
          </li>
        </ul>
      </section>

      <section class="pd-sec">
        <h3 class="pd-label">Recent output</h3>
        <div v-if="recent.length" class="pd-out"><LogTail :lines="recent" max-height="180px" /></div>
        <p v-else class="pd-dim">No output yet. Start the project to see it here.</p>
      </section>

    </div>

    <div v-else-if="tab === 'env'" class="pd-body" v-bind="tabPanelAttrs('pd', tab)">
      <div class="pd-envbar">
        <UiInput
          as="select"
          width="180px"
          aria-label="Environment used when the project starts"
          :model-value="activeName"
          :options="envOptions"
          :disabled="envDirty"
          :title="envDirty ? 'Save or undo your changes first' : undefined"
          @update:model-value="selectEnv"
        />
        <UiButton variant="quiet" :disabled="envDirty" @click="openNewEnv"><UiIcon name="plus" />New environment</UiButton>
        <span class="pd-sp"></span>
        <UiButton variant="quiet" @click="textMode ? toRows() : toText()">{{ textMode ? 'Edit as rows' : 'Edit as .env' }}</UiButton>
      </div>
      <UiStatusNotice
        v-if="restartToApply"
        tone="info"
        :title="`Running with ${runningEnv || 'Default'}`"
        :detail="`Restart ${p.name} to use ${activeName || 'Default'}. Nothing changes until then.`"
        retryable
        retry-label="Restart"
        @retry="actions.restart(p)"
      />
      <div class="pd-envhelp">
        <p class="pd-dim">Wins over the same name in the project's .env.</p>
        <button
          type="button"
          class="pd-help"
          :aria-expanded="envHelpOpen"
          aria-controls="pd-envhelp-text"
          aria-label="How environment variables work"
          @click="envHelpOpen = !envHelpOpen"
          @keydown.esc="envHelpOpen = false"
        >?</button>
      </div>
      <div v-if="envHelpOpen" id="pd-envhelp-text" class="pd-envhelp-text">
        <p>Kova sets these when it starts the project. Names you leave out still come from .env. Values stay on this PC.</p>
        <p v-if="activeName">An empty value uses Default (greyed). Type a value to override it for {{ activeName }}; clear it to go back to Default.</p>
        <p>Services › MySQL › Connect copies connection values for you.</p>
      </div>
      <div class="pd-envtools">
        <UiInput
          v-if="showEnvSearch"
          v-model="envSearch"
          class="pd-envsearch"
          type="search"
          mono
          aria-label="Search variables"
          :placeholder="`Search ${rows.length} variables`"
        />
        <span v-else class="pd-sp"></span>
        <UiButton :disabled="!envDirty || envSaving" @click="saveEnv(false)">Save</UiButton>
        <UiButton
          variant="primary"
          :disabled="!canRun(p) || envSaving || (!envDirty && isActive(state))"
          :loading="envSaving"
          @click="saveEnv(true)"
        >{{ isActive(state) ? 'Save & restart' : 'Save & start' }}</UiButton>
      </div>
      <UiStatusNotice
        v-if="pasteNote"
        :tone="pasteNote.skipped.length ? 'warn' : 'ok'"
        :title="pasteNote.title"
        :detail="pasteDetail"
        dismissible
        @dismiss="pasteNote = null"
      />
      <UiInput
        v-if="textMode"
        v-model="envText"
        as="textarea"
        :rows="12"
        mono
        :label="activeName ? `${activeName} · only the values that differ from Default` : 'One KEY=VALUE per line'"
        help="Comments (#), blank lines, export and quoted values are fine. PORT is set in Project settings."
        placeholder="APP_ENV=local"
      />
      <template v-else>
        <p v-if="showEnvSearch && envSearch.trim() && visibleRows.length === 0" class="pd-dim">No variable matches "{{ envSearch.trim() }}".</p>
        <template v-for="{ r, i } in visibleRows" :key="i">
        <h3 v-if="showGroups && i === 0 && defaultStart > 0" class="pd-env-group">Set in {{ activeName }}</h3>
        <h3 v-if="showGroups && i === defaultStart" class="pd-env-group">From Default <span>type a value to change it for {{ activeName }}</span></h3>
        <div class="pd-env">
          <UiInput
            v-model="r.k"
            mono
            :readonly="r.inh"
            :aria-label="`Name of variable ${i + 1}`"
            placeholder="NAME or paste KEY=VALUE lines"
            @paste="onPaste($event, i, 'k')"
          />
          <UiInput
            v-model="r.v"
            mono
            :aria-label="`Value for ${r.k || `variable ${i + 1}`}`"
            :placeholder="r.inh ? r.ph || '(empty in Default)' : 'value'"
            @paste="onPaste($event, i, 'v')"
          />
          <UiButton
            variant="quiet"
            :disabled="r.inh && r.v === ''"
            :aria-label="r.inh ? `Use Default for ${r.k}` : `Remove ${r.k || `variable ${i + 1}`}`"
            :title="r.inh ? 'Use the Default value' : undefined"
            @click="removeRow(i)"
          >&times;</UiButton>
        </div>
        </template>
        <div><UiButton @click="addRow"><UiIcon name="plus" />Add variable</UiButton></div>
      </template>
      <UiBanner v-if="envError" tone="err" title="Check the variables">{{ envError }}</UiBanner>
      <div v-if="activeName" class="pd-env-actions">
        <UiButton variant="quiet" class="pd-env-del" @click="deleteEnvOpen = true"><UiIcon name="trash" />Delete {{ activeName }}</UiButton>
      </div>
    </div>

    <div v-else class="pd-body" v-bind="tabPanelAttrs('pd', tab)">
      <LogTail v-if="last500.length" :lines="last500" max-height="calc(100vh - 300px)" />
      <p v-else class="pd-dim">No output yet in this session.</p>
      <div class="pd-env-actions">
        <span class="pd-dim">Last 500 lines</span>
        <UiButton @click="actions.viewLog(p)">Open in Logs</UiButton>
      </div>
    </div>

    <UiModal :open="newEnvOpen" title="New environment" size="sm" @close="newEnvOpen = false">
      <div class="pd-modal">
        <UiInput
          v-model="newEnvName"
          mono
          label="Name"
          placeholder="staging"
          :error="newEnvError || undefined"
          help="Letters, numbers, - and _. Shown on the project while it runs."
          :maxlength="40"
          @keydown.enter="createEnv"
        />
        <UiInput
          v-model="newEnvCopy"
          as="select"
          label="Copy changes from"
          :options="[{ value: '', label: 'Nothing (start from Default)' }, ...envs.map((e) => ({ value: e.name, label: e.name }))]"
          help="Default's variables always apply. Add only the values this environment changes."
        />
      </div>
      <template #footer>
        <UiButton variant="quiet" size="md" @click="newEnvOpen = false">Cancel</UiButton>
        <UiButton variant="primary" size="md" :disabled="!newEnvName.trim() || !!newEnvError" @click="createEnv">Create environment</UiButton>
      </template>
    </UiModal>
    <UiModal :open="deleteEnvOpen" :title="`Delete ${activeName}?`" size="sm" danger @close="deleteEnvOpen = false">
      <p class="pd-text">Its values are removed from Kova. {{ p.name }} uses Default the next time it starts. Default does not change.</p>
      <template #footer>
        <UiButton variant="quiet" size="md" @click="deleteEnvOpen = false">Cancel</UiButton>
        <UiButton variant="danger" size="md" @click="deleteEnv">Delete environment</UiButton>
      </template>
    </UiModal>

    <!-- Selalu di bawah panel, untuk semua tab. -->
    <div class="pd-open">
      <UiButton :disabled="!p.path" title="Open folder" @click="openIn('folder')"><UiIcon name="folder" /><span class="pd-open-label">Open folder</span></UiButton>
      <UiButton :disabled="!p.path" :title="terminalTitle" @click="openIn('terminal')"><UiIcon name="terminal" /><span class="pd-open-label">Terminal here</span></UiButton>
      <UiButton :disabled="!p.path" title="Open in VS Code" @click="openIn('vscode')"><UiIcon name="code" /><span class="pd-open-label">Open in VS Code</span></UiButton>
    </div>
  </div>
</template>

<style scoped>
.pd { display: flex; flex-direction: column; gap: var(--space-3); min-width: 0; min-height: 100%; container: pd / inline-size; }
.pd-head { display: flex; flex-direction: column; gap: var(--space-3); }
.pd-title { display: flex; flex-direction: column; gap: var(--space-1); }
.pd-name { margin: 0; font-size: var(--text-lg); font-weight: 600; color: var(--text); overflow-wrap: anywhere; }
.pd-actions { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: center; }
.pd-body { display: flex; flex-direction: column; gap: var(--space-4); font-size: var(--text-sm); }
.pd-grid { display: grid; grid-template-columns: 96px minmax(0, 1fr); gap: var(--space-2) var(--space-3); margin: 0; font-size: var(--text-sm); align-items: baseline; }
.pd-grid dt { color: var(--muted); }
.pd-grid dd { margin: 0; color: var(--text); min-width: 0; }
.pd-sec { display: flex; flex-direction: column; gap: var(--space-2); }
.pd-label { margin: 0; font-size: var(--text-xs); font-weight: 500; color: var(--muted); }
.pd-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-xs); }
.pd-list li { display: flex; gap: var(--space-2); align-items: baseline; }
.pd-mono { font-family: var(--font-mono); font-weight: 500; }
.pd-wrap { overflow-wrap: anywhere; }
.pd-dim { margin: 0; color: var(--faint); font-size: var(--text-xs); }
.pd-warn { color: var(--amber); }
.pd-note { margin: 0; color: var(--muted); font-size: var(--text-xs); }
.pd-lastline { font: 400 var(--text-xs) var(--font-mono); color: var(--red); overflow-wrap: anywhere; }
.pd-open {
  position: sticky;
  bottom: -16px;
  margin: auto -16px -16px;
  padding: 12px 16px;
  display: flex;
  gap: var(--space-2);
  border-top: 1px solid var(--border);
  background: var(--surface);
}
.pd-titlerow { display: flex; align-items: flex-start; gap: var(--space-2); }
.pd-titlerow .pd-title { flex: 1; min-width: 0; }
.pd-muted { color: var(--muted) !important; }
.pd-grid .pd-mono { font-size: var(--text-xs); }
.pd-rt { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
.pd-chips { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: var(--space-2); }
.pd-chips :deep(.ui-chip) { display: inline-flex; align-items: center; gap: 6px; }
.pd-out { padding: var(--space-2) var(--space-3); background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius-md); }
.pd-sp { flex: 1; }
.pd-envbar { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
.pd-modal { display: flex; flex-direction: column; gap: var(--space-4); }
.pd-text { margin: 0; line-height: 1.6; }
.pd-env-del { margin-right: auto; }
.pd-env { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto; gap: var(--space-2); align-items: center; }
.pd-env-group { margin: var(--space-2) 0 0; font-size: var(--text-xs); font-weight: 600; color: var(--muted); }
.pd-env-group span { margin-left: var(--space-2); font-weight: 400; color: var(--faint); }
.pd-env-actions { display: flex; gap: var(--space-2); align-items: center; justify-content: flex-end; }
.pd-env-actions .pd-dim { margin-right: auto; }
.pd-envhelp { display: flex; align-items: center; gap: var(--space-2); }
.pd-help {
  flex: none;
  width: 20px;
  height: 20px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 50%;
  background: transparent;
  color: var(--muted);
  font: 600 var(--text-xs) var(--font-sans);
  cursor: pointer;
}
.pd-help:hover, .pd-help[aria-expanded='true'] { color: var(--text); border-color: var(--accent); }
.pd-help:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.pd-envhelp-text {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg);
  color: var(--muted);
  font-size: var(--text-xs);
}
.pd-envhelp-text p { margin: 0; line-height: 1.5; }
/* Toolbar menempel di atas saat daftar variabel di-scroll. */
.pd-envtools {
  position: sticky;
  top: -16px;
  z-index: 1;
  margin: 0 -16px;
  padding: var(--space-2) 16px;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  background: var(--surface);
  border-bottom: 1px solid var(--border);
}
.pd-envsearch { flex: 1; min-width: 0; }
/* Panel sempit: pencarian variabel di baris sendiri; tiga tombol bawah tetap satu baris, tinggal ikon (nama tetap terbaca
   pembaca layar dan muncul sebagai tooltip). */
@container pd (max-width: 400px) {
  .pd-envtools { flex-wrap: wrap; }
  .pd-envsearch { flex-basis: 100%; }
  .pd-open-label { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); clip-path: inset(50%); white-space: nowrap; }
}
.pd-containers { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
.pd-containers li { display: flex; gap: var(--space-2); align-items: baseline; }
.pd-ok { color: var(--green); }
</style>
