<script setup lang="ts">
// Panel detail project (v3.4, canvas Projects / Flow C): kepala dengan status
// dan aksi, lalu tab Overview, Environment, Logs.
import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiButton from '@/components/ui/UiButton.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiMenu from '@/components/ui/UiMenu.vue'
import UiTabs from '@/components/ui/UiTabs.vue'
import UiInput from '@/components/ui/UiInput.vue'
import StatusDot from '@/components/ui/StatusDot.vue'
import UiStatusNotice from '@/components/ui/UiStatusNotice.vue'
import { parseDotenv, serializeDotenv, mergeRows, summarize, type SkippedLine } from '@/utils/dotenv'
import LogTail from '@/components/LogTail.vue'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import { useServices } from '@/composables/useServices'
import { useToast } from '@/composables/useToast'
import { useProjectRuntime, isActive } from '@/composables/useProjectRuntime'
import { useProjectActions, canRun, missingReason } from '@/composables/useProjectActions'
import type { Project, ToolCurrent } from '@/types/mise'

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
const up = computed(() => state.value === 'running' || state.value === 'unresponsive')

const statusLine = computed(() => actions.stateLabel(p.value) + (state.value === 'running' ? ` · :${p.value.port}` : ''))

// --- Overview ---------------------------------------------------------------
const tools = ref<ToolCurrent[] | null>(null)
async function loadTools(): Promise<void> {
  if (!p.value.path) {
    tools.value = []
    return
  }
  try {
    tools.value = await invoke<ToolCurrent[]>('mise_project_tools', { path: p.value.path })
  } catch {
    tools.value = []
  }
}
onMounted(loadTools)

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

function sourceLabel(t: ToolCurrent): string {
  if (t.scope === 'global') return 'global'
  return t.source?.split(/[\\/]/).pop() ?? ''
}

async function openIn(target: 'folder' | 'terminal' | 'vscode'): Promise<void> {
  try {
    await invoke('project_open_in', { target, path: p.value.path })
  } catch (err) {
    actions.setError(p.value.id, String(err))
  }
}

// --- Environment ------------------------------------------------------------
interface Row { k: string; v: string }
const rows = ref<Row[]>([])
const envError = ref('')
const envSaving = ref(false)

// v3.8.5 (canvas H4): "Edit as .env" dan paste banyak baris sekaligus.
const textMode = ref(false)
const envText = ref('')
const pasteNote = ref<{ title: string; skipped: SkippedLine[] } | null>(null)

function resetRows(): void {
  rows.value = Object.entries(p.value.env ?? {}).map(([k, v]) => ({ k, v }))
  if (textMode.value) envText.value = serializeDotenv(rows.value)
  envError.value = ''
}
watch(
  () => p.value.id,
  () => {
    textMode.value = false
    pasteNote.value = null
    resetRows()
  },
  { immediate: true },
)

function noteFor(title: string, skipped: SkippedLine[]): void {
  pasteNote.value = { title, skipped }
}
const pasteDetail = computed(() =>
  pasteNote.value?.skipped.map((s) => `Line ${s.line}: ${s.text} — ${s.reason}`).join('\n') || undefined,
)

function toText(): void {
  envText.value = serializeDotenv(rows.value)
  textMode.value = true
  pasteNote.value = null
}
function toRows(): void {
  const r = parseDotenv(envText.value)
  rows.value = r.entries
  textMode.value = false
  pasteNote.value = null
  if (r.skipped.length) noteFor(`${r.entries.length} variables · ${r.skipped.length} skipped`, r.skipped)
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
  rows.value = m.rows.length ? m.rows : [{ k: '', v: '' }]
  noteFor(summarize(m.added, m.replaced, r.skipped.length), r.skipped)
}

const envDirty = computed(() => {
  const cur = Object.entries(p.value.env ?? {})
  const src = textMode.value ? parseDotenv(envText.value).entries : rows.value
  const next = src.filter((r) => r.k.trim() !== '' || r.v !== '')
  return cur.length !== next.length || next.some((r, i) => cur[i]?.[0] !== r.k.trim() || cur[i]?.[1] !== r.v)
})

function validateEnv(): Record<string, string> | null {
  const env: Record<string, string> = {}
  for (const [i, r] of rows.value.entries()) {
    const k = r.k.trim()
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

async function saveEnv(thenRestart: boolean): Promise<void> {
  if (textMode.value) {
    const r = parseDotenv(envText.value)
    rows.value = r.entries
    if (r.skipped.length) noteFor(`${r.entries.length} variables · ${r.skipped.length} skipped`, r.skipped)
  }
  const env = validateEnv()
  if (!env) return
  envSaving.value = true
  try {
    await updateProject(p.value.id, { env })
    resetRows()
    const fresh = config.value.projects.find((x) => x.id === p.value.id)
    if (thenRestart && fresh) {
      await (isActive(state.value) ? actions.restart(fresh) : actions.start(fresh))
      tab.value = 'overview'
    } else {
      toast.show('Environment saved. It applies the next time the project starts.')
    }
  } finally {
    envSaving.value = false
  }
}

const tabs = computed(() => [
  { value: 'overview', label: 'Overview' },
  { value: 'env', label: 'Environment', badge: Object.keys(p.value.env ?? {}).length || undefined },
  { value: 'logs', label: 'Logs' },
])
</script>

<template>
  <div class="pd">
    <header class="pd-head">
      <div class="pd-title">
        <h2 class="pd-name">{{ p.name }}</h2>
        <StatusDot :state="state" :label="statusLine" />
      </div>
      <div class="pd-actions">
        <template v-if="!p.path">
          <UiButton variant="accent" @click="actions.editingId.value = p.id">Choose folder</UiButton>
        </template>
        <template v-else-if="!canRun(p)">
          <UiButton variant="accent" @click="actions.editingId.value = p.id">Project settings</UiButton>
        </template>
        <template v-else-if="up">
          <UiButton variant="accent" @click="actions.open(p)">Open localhost:{{ p.port }}</UiButton>
          <UiButton :disabled="busy" @click="actions.stop(p)">Stop</UiButton>
          <UiButton :disabled="busy" @click="actions.restart(p)">Restart</UiButton>
        </template>
        <template v-else-if="state === 'starting'">
          <UiButton disabled loading>Starting…</UiButton>
          <UiButton @click="actions.stop(p)">Stop</UiButton>
        </template>
        <UiButton v-else variant="success" :loading="busy" @click="actions.start(p)">Start</UiButton>
        <UiMenu :items="actions.menuItems(p)" :label="`More actions for ${p.name}`" @select="actions.onMenu(p, $event)" />
      </div>
    </header>

    <UiTabs v-model="tab" :tabs="tabs" label="Project details" />

    <!-- Overview -->
    <div v-if="tab === 'overview'" class="pd-body" role="tabpanel">
      <UiBanner v-if="error" tone="err" title="That did not work">{{ error }}</UiBanner>
      <UiBanner
        v-else-if="state === 'exited'"
        tone="err"
        :title="info?.exitCode != null ? `Stopped with exit code ${info.exitCode}` : 'Stopped'"
      >
        <code v-if="info?.lastLine" class="pd-lastline">{{ info.lastLine }}</code>
        <template #actions>
          <UiButton @click="actions.viewLog(p)">View log</UiButton>
          <UiButton @click="tab = 'env'">Environment</UiButton>
          <UiButton variant="accent" :loading="busy" @click="actions.start(p)">Restart</UiButton>
        </template>
      </UiBanner>
      <UiBanner v-else-if="state === 'unresponsive'" tone="warn" :title="`Running, but nothing answers on :${p.port}`">
        The app may listen on another port. Add {port} to the command so it uses the project's port.
        <template #actions>
          <UiButton @click="actions.editingId.value = p.id">Edit command</UiButton>
          <UiButton @click="actions.viewLog(p)">View log</UiButton>
        </template>
      </UiBanner>
      <p v-if="!canRun(p)" class="pd-note">{{ missingReason(p) }}</p>

      <dl class="pd-grid">
        <dt>Type</dt>
        <dd>{{ p.kind || 'Not detected' }}</dd>
        <dt>Command</dt>
        <dd class="pd-mono">{{ p.command || '—' }}</dd>
        <dt>Folder</dt>
        <dd class="pd-mono pd-wrap">{{ p.path || 'Not chosen yet' }}</dd>
      </dl>

      <section class="pd-sec">
        <h3 class="pd-label">Runtime</h3>
        <p v-if="tools === null" class="pd-dim">Checking…</p>
        <p v-else-if="tools.length === 0" class="pd-dim">No versions set by mise for this folder.</p>
        <ul v-else class="pd-list">
          <li v-for="t in tools" :key="t.tool">
            <span>{{ t.tool }}</span>
            <b class="pd-mono">{{ t.version }}</b>
            <span v-if="!t.installed" class="pd-warn">not installed</span>
            <span class="pd-dim">{{ sourceLabel(t) }}</span>
          </li>
        </ul>
      </section>

      <section class="pd-sec">
        <h3 class="pd-label">Services{{ profile ? ` · profile ${profile.name}` : '' }}</h3>
        <p v-if="!profile" class="pd-dim">No service profile. Pick one in Project settings.</p>
        <p v-else-if="profileServices.length === 0" class="pd-dim">This profile has no services.</p>
        <ul v-else class="pd-list">
          <li v-for="s in profileServices" :key="s.id"><StatusDot :state="s.state" :label="s.name" /></li>
        </ul>
      </section>

      <section class="pd-sec">
        <h3 class="pd-label">Recent output</h3>
        <LogTail v-if="recent.length" :lines="recent" max-height="180px" />
        <p v-else class="pd-dim">No output yet. Start the project to see it here.</p>
      </section>

      <div class="pd-open">
        <UiButton variant="quiet" :disabled="!p.path" @click="openIn('folder')">Open folder</UiButton>
        <UiButton variant="quiet" :disabled="!p.path" @click="openIn('terminal')">Terminal here</UiButton>
        <UiButton variant="quiet" :disabled="!p.path" @click="openIn('vscode')">Open in VS Code</UiButton>
      </div>
    </div>

    <!-- Environment -->
    <div v-else-if="tab === 'env'" class="pd-body" role="tabpanel">
      <div class="pd-env-head">
        <p class="pd-dim">Added on top of the environment from mise. Values stay on this PC.</p>
        <UiButton variant="quiet" @click="textMode ? toRows() : toText()">{{ textMode ? 'Edit as rows' : 'Edit as .env' }}</UiButton>
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
        label="One KEY=VALUE per line"
        help="Comments (#), blank lines, export and quoted values are fine. PORT is set in Project settings."
        placeholder="APP_ENV=local"
      />
      <template v-else>
        <div v-for="(r, i) in rows" :key="i" class="pd-env">
          <UiInput
            v-model="r.k"
            mono
            :aria-label="`Name of variable ${i + 1}`"
            placeholder="NAME or paste KEY=VALUE lines"
            @paste="onPaste($event, i, 'k')"
          />
          <UiInput
            v-model="r.v"
            mono
            :aria-label="`Value for ${r.k || `variable ${i + 1}`}`"
            placeholder="value"
            @paste="onPaste($event, i, 'v')"
          />
          <UiButton variant="quiet" :aria-label="`Remove ${r.k || `variable ${i + 1}`}`" @click="rows.splice(i, 1)">&times;</UiButton>
        </div>
        <div><UiButton @click="rows.push({ k: '', v: '' })">Add variable</UiButton></div>
      </template>
      <p v-if="profile" class="pd-dim">Tip: Services › Connect copies database and cache values for you.</p>
      <UiBanner v-if="envError" tone="err" title="Check the variables">{{ envError }}</UiBanner>
      <div class="pd-env-actions">
        <UiButton :disabled="!envDirty || envSaving" @click="saveEnv(false)">Save</UiButton>
        <UiButton
          variant="primary"
          :disabled="!canRun(p) || envSaving || (!envDirty && isActive(state))"
          :loading="envSaving"
          @click="saveEnv(true)"
        >{{ isActive(state) ? 'Save & restart' : 'Save & start' }}</UiButton>
      </div>
    </div>

    <!-- Logs -->
    <div v-else class="pd-body" role="tabpanel">
      <LogTail v-if="last500.length" :lines="last500" max-height="calc(100vh - 300px)" />
      <p v-else class="pd-dim">No output yet in this session.</p>
      <div class="pd-env-actions">
        <span class="pd-dim">Last 500 lines</span>
        <UiButton @click="actions.viewLog(p)">Open in Logs</UiButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pd { display: flex; flex-direction: column; gap: var(--space-3); min-width: 0; }
.pd-head { display: flex; flex-direction: column; gap: var(--space-3); }
.pd-title { display: flex; flex-direction: column; gap: var(--space-1); }
.pd-name { margin: 0; font-size: var(--text-lg); font-weight: 600; color: var(--text); overflow-wrap: anywhere; }
.pd-actions { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: center; }
.pd-body { display: flex; flex-direction: column; gap: var(--space-4); font-size: var(--text-sm); }
.pd-grid { display: grid; grid-template-columns: max-content 1fr; gap: var(--space-2) var(--space-4); margin: 0; font-size: var(--text-xs); }
.pd-grid dt { color: var(--faint); }
.pd-grid dd { margin: 0; color: var(--text); min-width: 0; }
.pd-sec { display: flex; flex-direction: column; gap: var(--space-2); }
.pd-label { margin: 0; font-size: var(--text-xs); font-weight: 500; color: var(--faint); text-transform: uppercase; letter-spacing: 0.06em; }
.pd-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-xs); }
.pd-list li { display: flex; gap: var(--space-2); align-items: baseline; }
.pd-mono { font-family: var(--font-mono); font-weight: 500; }
.pd-wrap { overflow-wrap: anywhere; }
.pd-dim { margin: 0; color: var(--faint); font-size: var(--text-xs); }
.pd-warn { color: var(--amber); }
.pd-note { margin: 0; color: var(--muted); font-size: var(--text-xs); }
.pd-lastline { font: 400 var(--text-xs) var(--font-mono); color: var(--red); overflow-wrap: anywhere; }
.pd-open { display: flex; flex-wrap: wrap; gap: var(--space-2); border-top: 1px solid var(--border); padding-top: var(--space-3); }
.pd-env-head { display: flex; align-items: center; gap: var(--space-2); }
.pd-env-head .pd-dim { flex: 1; margin: 0; }
.pd-env { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto; gap: var(--space-2); align-items: center; }
.pd-env-actions { display: flex; gap: var(--space-2); align-items: center; justify-content: flex-end; }
.pd-env-actions .pd-dim { margin-right: auto; }
</style>
