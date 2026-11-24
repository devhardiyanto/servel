<script setup lang="ts">
// Runtimes per tool: tab PHP / Node / Go / Python,
// tabel versi (global, dipakai project mana), menu ⋯ per versi, dialog
// "Use in project", dan install dengan progres serta Cancel.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiIcon from '@/components/ui/UiIcon.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiChip from '@/components/ui/UiChip.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiMenu, { type MenuItem } from '@/components/ui/UiMenu.vue'
import UiModal from '@/components/ui/UiModal.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import PhpExtensionsPanel from '@/components/PhpExtensionsPanel.vue'
import { useConfig } from '@/composables/useConfig'
import { useVersionContext } from '@/composables/useVersionContext'
import { useRuntimeLocation } from '@/composables/useRuntimeLocation'
import { useLogs } from '@/composables/useLogs'
import { useToast } from '@/composables/useToast'
import { useRuntimeInstall, toolLabel, freshKey, type RuntimeTool } from '@/composables/useRuntimeInstall'
import AddToolModal from '@/components/AddToolModal.vue'
import { wslDistro } from '@/utils/wsl'
import { frameworkTool } from '@/utils/framework'

type Tool = RuntimeTool
interface VersionRow { version: string; installed: boolean; global: boolean; usedBy: string[] }
interface Overview { tool: string; global: string | null; versions: VersionRow[] }

const BUILTIN: { value: Tool; label: string; example: string }[] = [
  { value: 'php', label: 'PHP', example: '8.4' },
  { value: 'node', label: 'Node', example: '22' },
  { value: 'go', label: 'Go', example: '1.23' },
  { value: 'python', label: 'Python', example: '3.12' },
]

const { config, setActiveProject, removeExtraTool } = useConfig()
/** Tab: tool bawaan + tool dari Add tool (Flow L5–L6). */
const TOOLS = computed(() => [
  ...BUILTIN,
  ...(config.value.extraTools ?? []).filter((t) => !BUILTIN.some((b) => b.value === t)).map((t) => ({ value: t, label: toolLabel(t), example: 'latest' })),
])
const isExtra = computed(() => !BUILTIN.some((b) => b.value === tool.value))
const addToolOpen = ref(false)

// Lokasi (Flow L1–L4): '' = PC ini, selain itu nama distro WSL.
const distros = ref<string[]>([])
const { loc, distroMise } = useRuntimeLocation()
const distroArg = computed(() => loc.value || null)
const installingMise = ref(false)
const locOptions = computed(() => [{ value: '', label: 'Windows' }, ...distros.value.map((d) => ({ value: d, label: `WSL · ${d}` }))])
async function checkDistroMise(): Promise<void> {
  distroMise.value = undefined
  if (!loc.value) return
  try {
    distroMise.value = await invoke<string | null>('wsl_mise_version', { distro: loc.value })
  } catch {
    distroMise.value = null
  }
}
async function installMiseInDistro(): Promise<void> {
  installingMise.value = true
  actionError.value = ''
  try {
    await invoke('wsl_mise_install', { distro: loc.value })
    toast.show(`mise is installed in ${loc.value}`)
    await checkDistroMise()
    await load()
  } catch (err) {
    actionError.value = String(err)
  } finally {
    installingMise.value = false
  }
}
watch(loc, async () => {
  await checkDistroMise()
  if (!loc.value || distroMise.value) void load()
})
async function removeToolTab(): Promise<void> {
  const t = tool.value
  tool.value = 'php'
  await removeExtraTool(t)
  toast.show(`Removed the ${toolLabel(t)} tab. Installed versions stay until you uninstall them.`)
}
const versionCtx = useVersionContext()
const { lines } = useLogs()
const toast = useToast()

const tool = ref<Tool>('php')
const meta = computed(() => TOOLS.value.find((t) => t.value === tool.value) ?? { value: tool.value, label: toolLabel(tool.value), example: 'latest' })
/** Cache per `lokasi|tool`. */
const data = ref<Partial<Record<string, Overview>>>({})
const key = (t: string = tool.value): string => `${loc.value}|${t}`
const loading = ref(false)
const loadError = ref('')
const busy = ref(false)
const actionError = ref('')

const projectsWithFolder = computed(() =>
  config.value.projects.filter((p) => p.path && (loc.value ? wslDistro(p.path) === loc.value : true)),
)

async function load(t: Tool = tool.value): Promise<void> {
  if (loc.value && !distroMise.value) return
  const k = key(t)
  if (!data.value[k]) loading.value = true
  loadError.value = ''
  try {
    data.value = {
      ...data.value,
      [k]: await invoke<Overview>('runtime_overview', {
        tool: t,
        projects: projectsWithFolder.value.map((p) => ({ name: p.name, path: p.path })),
        distro: distroArg.value,
      }),
    }
  } catch (err) {
    loadError.value = String(err)
  } finally {
    loading.value = false
  }
}
watch(tool, (t) => void load(t))

onMounted(async () => {
  await versionCtx.refresh()
  // Runtimes tidak punya pemilih project; versi diatur global atau per
  // project lewat "Use in project". Tray mengikuti konteks ini, jadi
  // kembalikan ke global supaya tray tidak menulis ke mise.toml project lama.
  if (config.value.activeProjectId) await setActiveProject(null)
  distros.value = await invoke<string[]>('wsl_distros').catch(() => [])
  // Distro yang tadi dipilih sudah tidak ada: kembali ke Windows.
  if (loc.value && !distros.value.includes(loc.value)) loc.value = ''
  else if (loc.value) await checkDistroMise()
  await load()
  // Versi global diganti dari popup tray: data tab itu sudah basi.
  unlistenGlobal = await listen<{ tool: Tool }>('runtime-global-changed', (e) => {
    const { [`|${e.payload.tool}`]: _stale, ...rest } = data.value
    data.value = rest
    if (e.payload.tool === tool.value && !loc.value) void load()
    void versionCtx.refresh()
  })
})
let unlistenGlobal: UnlistenFn | null = null
onUnmounted(() => unlistenGlobal?.())

const rows = computed(() => data.value[key()]?.versions ?? [])
const rtInstall = useRuntimeInstall()
const isFresh = (version: string): boolean => rtInstall.fresh.value.has(freshKey(tool.value, version, distroArg.value))

// --- Install -----------------------------------------------------------------
// The job lives in useRuntimeInstall, so it survives leaving this page.
const newVersion = ref('')
const installing = computed(() => rtInstall.current.value)
const installLine = computed(() => {
  if (!installing.value) return ''
  const l = [...lines.value].reverse().find((x) => x.src === 'MISE')
  return l?.text ?? ''
})

async function install(version = newVersion.value.trim()): Promise<void> {
  if (!version || installing.value) return
  actionError.value = ''
  const r = await rtInstall.install(tool.value, version, distroArg.value)
  if (r?.ok && newVersion.value.trim() === version) newVersion.value = ''
}

// Reload when an install finishes, including one that ended while this page
// was closed (onMounted loads anyway in that case).
watch(rtInstall.lastResult, (r) => {
  if (!r) return
  if (!r.ok && !r.cancelled) actionError.value = r.error
  const { [`${r.distro ?? ''}|${r.tool}`]: _stale, ...rest } = data.value
  data.value = rest
  if (r.tool === tool.value && (r.distro ?? '') === loc.value) void load()
  if (r.madeGlobal) void versionCtx.refresh()
  if (r.ok && r.tool === 'php') {
    const v = [...rtInstall.fresh.value].filter((k) => k.startsWith('php@')).pop()?.slice(4)
    if (v) extFocus.value = v
  }
})

async function cancelInstall(): Promise<void> {
  await rtInstall.cancel()
}

// --- Aksi per versi -------------------------------------------------------------
function menuItems(r: VersionRow): MenuItem[] {
  if (!r.installed) return [{ id: 'install', label: `Install ${meta.value.label} ${r.version}` }]
  return [
    { id: 'global', label: 'Set as global', disabled: r.global },
    { id: 'use', label: 'Use in project…', disabled: projectsWithFolder.value.length === 0 },
    {
      id: 'remove',
      label: `Remove ${meta.value.label} ${r.version}…`,
      separator: true,
      danger: true,
      disabled: r.global || r.usedBy.length > 0,
      hint: r.global ? 'global' : r.usedBy.length ? 'in use' : undefined,
    },
  ]
}

async function run(fn: () => Promise<unknown>, done: string): Promise<void> {
  busy.value = true
  actionError.value = ''
  try {
    await fn()
    toast.show(done)
    await Promise.all([load(), versionCtx.refresh()])
  } catch (err) {
    actionError.value = String(err)
  } finally {
    busy.value = false
  }
}

/** PHP version the extensions panel shows after an action on it. */
const extFocus = ref<string | null>(null)
const usingRow = ref<VersionRow | null>(null)
const usePick = ref('')
const useShowAll = ref(false)
/** Projects of this tool: detected kind, or a mise.toml that already pins it. */
const useCandidates = computed(() => {
  const all = projectsWithFolder.value
  if (useShowAll.value) return all
  return all.filter((p) => frameworkTool(p.kind) === tool.value || !!currentFor(p.name))
})
watch(usingRow, (r) => {
  useShowAll.value = false
  usePick.value = r ? (useCandidates.value.find((p) => currentFor(p.name) !== r.version)?.id ?? useCandidates.value[0]?.id ?? '') : ''
})
const removingRow = ref<VersionRow | null>(null)

function onMenu(r: VersionRow, id: string): void {
  if (id === 'install') void install(r.version)
  else if (id === 'global') {
    if (tool.value === 'php') extFocus.value = r.version
    void run(() => invoke('runtime_set_global', { tool: tool.value, version: r.version, distro: distroArg.value }), `${meta.value.label} ${r.version} is now the global version${loc.value ? ` in ${loc.value}` : ''}`)
  } else if (id === 'use') usingRow.value = r
  else if (id === 'remove') removingRow.value = r
}

function currentFor(name: string): string {
  return rows.value.find((x) => x.usedBy.includes(name))?.version ?? ''
}

async function useIn(projectId = usePick.value): Promise<void> {
  const r = usingRow.value
  const p = config.value.projects.find((x) => x.id === projectId)
  if (!r || !p) return
  usingRow.value = null
  if (tool.value === 'php') extFocus.value = r.version
  await run(
    () => invoke('runtime_use_in', { tool: tool.value, version: r.version, path: p.path, distro: distroArg.value }),
    `${p.name} now uses ${meta.value.label} ${r.version}. Restart it to apply.`,
  )
}

async function confirmRemove(): Promise<void> {
  const r = removingRow.value
  if (!r) return
  removingRow.value = null
  await run(() => invoke('runtime_uninstall', { tool: tool.value, version: r.version, distro: distroArg.value }), `Removed ${meta.value.label} ${r.version}`)
}

const tabs = computed(() => TOOLS.value.map((t) => ({ value: t.value, label: t.label })))
</script>

<template>
  <div class="rtt">
    <!-- Judul dan pilihan tool satu baris. -->
    <div class="rtt-head">
      <h1 class="rtt-title">Runtimes</h1>
      <UiSegmented :model-value="tool" :options="tabs" label="Tool" @update:model-value="tool = $event as Tool" />
      <UiButton variant="quiet" @click="addToolOpen = true"><UiIcon name="plus" />Add tool</UiButton>
      <UiButton v-if="isExtra" variant="quiet" @click="removeToolTab">Remove tab</UiButton>
      <span class="rtt-sp"></span>
      <UiSegmented v-if="distros.length" v-model="loc" :options="locOptions" label="Where" />
      <span class="rtt-via">via mise {{ loc ? (distroMise ?? '') : (versionCtx.ctx.value?.miseVersion ?? '') }}</span>
    </div>

    <!-- Versi di kiri, PHP extensions sebagai panel di kanan. -->
    <div class="rtt-cols" :class="{ 'rtt-cols--split': tool === 'php' && !loc }">
    <div class="rtt-main">
    <UiBanner v-if="versionCtx.untrustedPath.value" tone="warn" title="mise.toml is not trusted yet">
      <code class="rtt-mono">{{ versionCtx.untrustedPath.value }}</code>: mise ignores this file until you trust it.
      <template #actions><UiButton @click="versionCtx.trust(versionCtx.untrustedPath.value!)">Trust</UiButton></template>
    </UiBanner>

    <EmptyState v-if="loc && distroMise === null" :title="`mise is not installed in ${loc}`" icon="lines">
      Kova installs and switches versions in the distro with mise, like on Windows. It goes into your home folder there (~/.local/bin); nothing else changes.
      <code class="rtt-mono">curl https://mise.run | sh</code>
      <template #actions>
        <UiButton variant="accent" :loading="installingMise" @click="installMiseInDistro">Install mise in {{ loc }}</UiButton>
      </template>
    </EmptyState>
    <div v-else-if="loc && distroMise === undefined" class="rtt-loading" aria-busy="true">
      <span class="rtt-dim">Checking {{ loc }}…</span>
    </div>
    <div v-else-if="loading" class="rtt-loading" aria-busy="true">
      <span class="rtt-dim">Loading installed {{ meta.label }} versions…</span>
      <SkeletonBox v-for="n in 3" :key="n" width="100%" height="36px" />
    </div>
    <UiBanner v-else-if="loadError" tone="err" :title="`Could not read ${meta.label} versions`">
      {{ loadError }}
      <template #actions><UiButton @click="load()">Retry</UiButton></template>
    </UiBanner>

    <template v-else>
      <EmptyState v-if="rows.length === 0" :title="`No ${meta.label} versions yet`" icon="lines">
        Install one here, or add {{ tool }} to a project's mise.toml. Kova installs it with mise.
      </EmptyState>
      <table v-else class="rtt-table">
        <thead>
          <tr><th scope="col">Version</th><th scope="col">Scope</th><th scope="col">Used by</th><th scope="col"><span class="sr-only">Actions</span></th></tr>
        </thead>
        <tbody>
          <tr v-for="r in rows" :key="r.version">
            <td class="rtt-mono rtt-ver">{{ r.version }}</td>
            <td>
              <UiChip v-if="r.global" tone="accent">global</UiChip>
              <UiChip v-if="isFresh(r.version)" tone="ok">new</UiChip>
              <UiChip v-if="!r.installed" tone="warn">not installed</UiChip>
            </td>
            <td class="rtt-used">{{ r.usedBy.length ? r.usedBy.join(', ') : 'Not used' }}</td>
            <td class="rtt-act">
              <!-- Versi yang diminta project tetapi belum terpasang: aksi utamanya langsung terlihat. -->
              <UiButton
                v-if="!r.installed"
                size="sm"
                :disabled="!!installing || busy"
                :aria-label="`Install ${meta.label} ${r.version}`"
                @click="install(r.version)"
              >Install</UiButton>
              <UiMenu v-else :items="menuItems(r)" :label="`${meta.label} ${r.version} actions`" @select="onMenu(r, $event)" />
            </td>
          </tr>
        </tbody>
      </table>
    </template>

    <UiBanner v-if="installing" tone="info" :title="`Installing ${toolLabel(installing.tool)} ${installing.version}${installing.distro ? ` in ${installing.distro}` : ''}`">
      <code class="rtt-mono">{{ installLine || 'starting mise…' }}</code>
      <template #actions><UiButton @click="cancelInstall">Cancel</UiButton></template>
    </UiBanner>
    <UiBanner v-if="actionError" tone="err" :title="`mise could not finish the ${meta.label} change`">{{ actionError }}</UiBanner>

    <div v-if="!loc || distroMise" class="rtt-install">
      <span class="rtt-label" aria-hidden="true">Install version</span>
      <UiInput
        v-model="newVersion"
        width="160px"
        mono
        :placeholder="`e.g. ${meta.example}`"
        :aria-label="`Install ${meta.label} version`"
        :disabled="!!installing"
        @keydown.enter="install()"
      />
      <UiButton variant="accent" :disabled="!newVersion.trim() || !!installing || busy" @click="install()">Install</UiButton>
      <span class="rtt-dim">Used by = projects whose mise.toml asks for this version. Global = the version outside any project.</span>
    </div>
    </div>

    <aside v-if="tool === 'php' && !loc" class="rtt-side" aria-label="PHP extensions">
      <PhpExtensionsPanel class="rtt-ext" :focus-version="extFocus" />
    </aside>
    <!-- Canvas L8: php.ini di distro belum diatur Kova; jelaskan, jangan tampilkan milik Windows. -->
    <aside v-else-if="tool === 'php' && loc" class="rtt-side rtt-wslext" aria-labelledby="rtt-wslext-h">
      <h2 id="rtt-wslext-h" class="rtt-wslext__h">PHP extensions in {{ loc }}</h2>
      <p>
        Kova manages extensions only for PHP on Windows. The PHP versions in {{ loc }} keep their own php.ini,
        and Kova does not change it.
      </p>
      <p>
        To turn an extension on there, run <code>php --ini</code> in {{ loc }} to see which php.ini that PHP reads,
        edit it, then restart the project.
      </p>
    </aside>
    </div>

    <AddToolModal :open="addToolOpen" :distros="distros" @close="addToolOpen = false" @added="(t) => (tool = t)" />
    <UiModal :open="!!usingRow" :title="usingRow ? `Use ${meta.label} ${usingRow.version} in a project` : ''" @close="usingRow = null">
      <p class="rtt-text">Kova writes <code class="rtt-mono">{{ tool }} = "{{ usingRow?.version }}"</code> to the project's mise.toml.</p>
      <div class="rtt-pick" role="radiogroup" :aria-label="`Project for ${meta.label} ${usingRow?.version}`">
        <label v-for="p in useCandidates" :key="p.id" class="rtt-pick__row">
          <input v-model="usePick" type="radio" name="rtt-use" :value="p.id" />
          <span class="rtt-pick__lbl">
            <span class="rtt-pick__name">{{ p.name }}</span>
            <span class="rtt-dim">{{ currentFor(p.name) ? `now ${meta.label} ${currentFor(p.name)} · mise.toml` : `uses the global ${meta.label}` }}</span>
          </span>
        </label>
        <p v-if="useCandidates.length === 0" class="rtt-dim">No project uses {{ meta.label }} yet.</p>
      </div>
      <p class="rtt-dim">
        <template v-if="!useShowAll">Only projects that use {{ meta.label }} are listed. </template>The project restarts only when you say so.
        <button v-if="!useShowAll && useCandidates.length < projectsWithFolder.length" type="button" class="rtt-link" @click="useShowAll = true">Show all projects</button>
      </p>
      <template #footer>
        <UiButton variant="quiet" size="md" @click="usingRow = null">Cancel</UiButton>
        <UiButton variant="primary" size="md" :disabled="!usePick" @click="useIn()">Use {{ meta.label }} {{ usingRow?.version }}</UiButton>
      </template>
    </UiModal>

    <UiModal :open="!!removingRow" :title="removingRow ? `Remove ${meta.label} ${removingRow.version}?` : ''" size="sm" danger @close="removingRow = null">
      <p class="rtt-text">mise uninstalls it from this PC. No project or the global config asks for it. You can install it again later.</p>
      <template #footer>
        <UiButton size="md" data-autofocus @click="removingRow = null">Cancel</UiButton>
        <UiButton variant="danger" size="md" @click="confirmRemove">Remove</UiButton>
      </template>
    </UiModal>
  </div>
</template>

<style scoped>
.rtt-title { margin: 0; font-size: var(--text-xl); letter-spacing: -0.015em; font-weight: 600; color: var(--text); }
.rtt { display: flex; flex-direction: column; gap: var(--space-4); container-type: inline-size; }
.rtt-head { display: flex; align-items: center; gap: var(--space-4); flex-wrap: wrap; }
.rtt-via { margin-left: auto; font: 400 var(--text-xs) var(--font-mono); color: var(--faint); }
.rtt-loading { display: flex; flex-direction: column; gap: var(--space-2); }
.rtt-table { width: 100%; border-collapse: collapse; font-size: var(--text-sm); }
.rtt-table th { text-align: left; font-size: var(--text-xs); font-weight: 500; color: var(--faint); padding: 0 var(--space-3) var(--space-2); }
.rtt-table td { padding: var(--space-2) var(--space-3); border-top: 1px solid var(--border); vertical-align: middle; }
.rtt-table td:first-child, .rtt-table th:first-child { padding-left: 0; }
.rtt-ver { font-size: var(--text-md) !important; font-weight: 600; }
.rtt-used { color: var(--muted); }
.rtt-act { text-align: right; width: 72px; }
.rtt-wslext { display: flex; flex-direction: column; gap: var(--space-2); font-size: var(--text-sm); color: var(--muted); line-height: 1.55; }
.rtt-wslext p { margin: 0; }
.rtt-wslext code { font-family: var(--font-mono); color: var(--text); }
.rtt-wslext__h { margin: 0; font-size: var(--text-sm); font-weight: 600; color: var(--text); }
.rtt-mono { font-family: var(--font-mono); font-size: var(--text-xs); }
.rtt-dim { color: var(--faint); font-size: var(--text-xs); }
.rtt-install { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); }
.rtt-install .rtt-dim { flex-basis: 100%; }
.rtt-label { font-size: var(--text-xs); font-weight: 500; color: var(--muted); }
.rtt-cols { display: flex; flex-direction: column; gap: var(--space-6); min-width: 0; }
.rtt-main { display: flex; flex-direction: column; gap: var(--space-4); min-width: 0; }
@container (min-width: 860px) {
  .rtt-cols--split { display: grid; grid-template-columns: minmax(0, 1fr) minmax(360px, 440px); align-items: start; gap: var(--space-5); }
}
.rtt-side { padding: var(--space-4); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); min-width: 0; }
.rtt-ext { margin: 0; }
.rtt-text { margin: 0 0 var(--space-3); line-height: 1.6; }
.rtt-pick { margin: 0 0 var(--space-3); display: flex; flex-direction: column; gap: var(--space-2); }
.rtt-pick__row {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-3);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface2);
  cursor: pointer;
}
.rtt-pick__row:hover, .rtt-pick__row:has(input:checked) { border-color: var(--accent); }
.rtt-pick__row input { margin: 3px 0 0; accent-color: var(--accent); }
.rtt-pick__lbl { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.rtt-link { background: none; border: 0; padding: 0; margin-left: var(--space-1); color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; }
.rtt-pick__name { font-weight: 600; }
.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
.rtt-sp { flex: 1; }
</style>
