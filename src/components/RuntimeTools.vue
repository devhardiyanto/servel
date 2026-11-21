<script setup lang="ts">
// Runtimes per tool (v3.5, canvas Flow E): tab PHP / Node / Go / Python,
// tabel versi (global, dipakai project mana), menu ⋯ per versi, dialog
// "Use in project", dan install dengan progres serta Cancel.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import UiTabs from '@/components/ui/UiTabs.vue'
import UiButton from '@/components/ui/UiButton.vue'
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
import { useLogs } from '@/composables/useLogs'
import { useToast } from '@/composables/useToast'

type Tool = 'php' | 'node' | 'go' | 'python'
interface VersionRow { version: string; installed: boolean; global: boolean; usedBy: string[] }
interface Overview { tool: string; global: string | null; versions: VersionRow[] }

const TOOLS: { value: Tool; label: string; example: string }[] = [
  { value: 'php', label: 'PHP', example: '8.4' },
  { value: 'node', label: 'Node', example: '22' },
  { value: 'go', label: 'Go', example: '1.23' },
  { value: 'python', label: 'Python', example: '3.12' },
]

const { config, setActiveProject } = useConfig()
const versionCtx = useVersionContext()
const { lines } = useLogs()
const toast = useToast()

const tool = ref<Tool>('php')
const meta = computed(() => TOOLS.find((t) => t.value === tool.value)!)
const data = ref<Partial<Record<Tool, Overview>>>({})
const loading = ref(false)
const loadError = ref('')
const busy = ref(false)
const actionError = ref('')

const projectsWithFolder = computed(() => config.value.projects.filter((p) => p.path))

async function load(t: Tool = tool.value): Promise<void> {
  if (!data.value[t]) loading.value = true
  loadError.value = ''
  try {
    data.value = {
      ...data.value,
      [t]: await invoke<Overview>('runtime_overview', {
        tool: t,
        projects: projectsWithFolder.value.map((p) => ({ name: p.name, path: p.path })),
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
  // v3.5: Runtimes tidak lagi punya "Versions for"; versi diatur global atau
  // per project lewat "Use in project". Tray mengikuti konteks ini, jadi
  // kembalikan ke global supaya tray tidak menulis ke mise.toml project lama.
  if (config.value.activeProjectId) await setActiveProject(null)
  await load()
  // Versi global diganti dari popup tray: data tab itu sudah basi.
  unlistenGlobal = await listen<{ tool: Tool }>('runtime-global-changed', (e) => {
    const { [e.payload.tool]: _stale, ...rest } = data.value
    data.value = rest
    if (e.payload.tool === tool.value) void load()
    void versionCtx.refresh()
  })
})
let unlistenGlobal: UnlistenFn | null = null
onUnmounted(() => unlistenGlobal?.())

const rows = computed(() => data.value[tool.value]?.versions ?? [])

// --- Install -----------------------------------------------------------------
const newVersion = ref('')
const installing = ref<string | null>(null)
const installLine = computed(() => {
  if (!installing.value) return ''
  const l = [...lines.value].reverse().find((x) => x.src === 'MISE')
  return l?.text ?? ''
})

async function install(version = newVersion.value.trim()): Promise<void> {
  if (!version || installing.value) return
  actionError.value = ''
  installing.value = version
  try {
    await invoke('runtime_install_version', { tool: tool.value, version })
    newVersion.value = ''
    toast.show(`Installed ${meta.value.label} ${version}`)
  } catch (err) {
    if (!String(err).includes('cancelled')) actionError.value = String(err)
    else toast.show(`Install of ${meta.value.label} ${version} cancelled`)
  } finally {
    installing.value = null
    await load()
  }
}

async function cancelInstall(): Promise<void> {
  await invoke('runtime_install_cancel')
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

const usingRow = ref<VersionRow | null>(null)
const removingRow = ref<VersionRow | null>(null)

function onMenu(r: VersionRow, id: string): void {
  if (id === 'install') void install(r.version)
  else if (id === 'global')
    void run(() => invoke('runtime_set_global', { tool: tool.value, version: r.version }), `${meta.value.label} ${r.version} is now the global version`)
  else if (id === 'use') usingRow.value = r
  else if (id === 'remove') removingRow.value = r
}

function currentFor(name: string): string {
  return rows.value.find((x) => x.usedBy.includes(name))?.version ?? ''
}

async function useIn(projectId: string): Promise<void> {
  const r = usingRow.value
  const p = config.value.projects.find((x) => x.id === projectId)
  if (!r || !p) return
  usingRow.value = null
  await run(
    () => invoke('runtime_use_in', { tool: tool.value, version: r.version, path: p.path }),
    `${p.name} now uses ${meta.value.label} ${r.version}. Restart it to apply.`,
  )
}

async function confirmRemove(): Promise<void> {
  const r = removingRow.value
  if (!r) return
  removingRow.value = null
  await run(() => invoke('runtime_uninstall', { tool: tool.value, version: r.version }), `Removed ${meta.value.label} ${r.version}`)
}

const tabs = computed(() => TOOLS.map((t) => ({ value: t.value, label: t.label })))
</script>

<template>
  <div class="rtt">
    <h1 class="rtt-title">Runtimes</h1>
    <div class="rtt-head">
      <UiTabs v-model="tool" :tabs="tabs" label="Tools" />
      <span class="rtt-via">via mise {{ versionCtx.ctx.value?.miseVersion ?? '' }}</span>
    </div>

    <!-- v3.8.6 (canvas Runtimes): versi di kiri, PHP extensions sebagai panel di kanan. -->
    <div class="rtt-cols" :class="{ 'rtt-cols--split': tool === 'php' }">
    <div class="rtt-main">
    <UiBanner v-if="versionCtx.untrustedPath.value" tone="warn" title="mise.toml is not trusted yet">
      <code class="rtt-mono">{{ versionCtx.untrustedPath.value }}</code> — mise ignores this file until you trust it.
      <template #actions><UiButton @click="versionCtx.trust(versionCtx.untrustedPath.value!)">Trust</UiButton></template>
    </UiBanner>

    <div v-if="loading" class="rtt-loading" aria-busy="true">
      <span class="rtt-dim">Reading mise ls…</span>
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
              <UiChip v-if="!r.installed" tone="warn">not installed</UiChip>
            </td>
            <td class="rtt-used">{{ r.usedBy.length ? r.usedBy.join(', ') : '—' }}</td>
            <td class="rtt-act">
              <UiMenu :items="menuItems(r)" :label="`${meta.label} ${r.version} actions`" @select="onMenu(r, $event)" />
            </td>
          </tr>
        </tbody>
      </table>
    </template>

    <UiBanner v-if="installing" tone="info" :title="`Installing ${meta.label} ${installing}`">
      <code class="rtt-mono">{{ installLine || 'starting mise…' }}</code>
      <template #actions><UiButton @click="cancelInstall">Cancel</UiButton></template>
    </UiBanner>
    <UiBanner v-if="actionError" tone="err" title="That did not work">{{ actionError }}</UiBanner>

    <div class="rtt-install">
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

    <aside v-if="tool === 'php'" class="rtt-side" aria-label="PHP extensions">
      <PhpExtensionsPanel class="rtt-ext" />
    </aside>
    </div>

    <UiModal :open="!!usingRow" :title="usingRow ? `Use ${meta.label} ${usingRow.version} in a project` : ''" @close="usingRow = null">
      <p class="rtt-text">Kova writes <code class="rtt-mono">{{ tool }} = "{{ usingRow?.version }}"</code> to the project's mise.toml.</p>
      <ul class="rtt-pick">
        <li v-for="p in projectsWithFolder" :key="p.id">
          <button type="button" class="rtt-pick__btn" @click="useIn(p.id)">
            <span class="rtt-pick__name">{{ p.name }}</span>
            <span class="rtt-dim">{{ currentFor(p.name) ? `now ${meta.label} ${currentFor(p.name)} · mise.toml` : `uses the global ${meta.label}` }}</span>
          </button>
        </li>
      </ul>
      <p class="rtt-dim">The project restarts only when you say so.</p>
      <template #footer><UiButton variant="quiet" size="md" @click="usingRow = null">Cancel</UiButton></template>
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
.rtt-head { display: flex; align-items: center; gap: var(--space-4); border-bottom: 1px solid var(--border); }
.rtt-via { margin-left: auto; font: 400 var(--text-xs) var(--font-mono); color: var(--faint); }
.rtt-loading { display: flex; flex-direction: column; gap: var(--space-2); }
.rtt-table { width: 100%; border-collapse: collapse; font-size: var(--text-sm); }
.rtt-table th { text-align: left; font-size: var(--text-xs); font-weight: 500; color: var(--faint); padding: 0 var(--space-3) var(--space-2); }
.rtt-table td { padding: var(--space-2) var(--space-3); border-top: 1px solid var(--border); vertical-align: middle; }
.rtt-table td:first-child, .rtt-table th:first-child { padding-left: 0; }
.rtt-ver { font-size: var(--text-md) !important; font-weight: 600; }
.rtt-used { color: var(--muted); }
.rtt-act { text-align: right; width: 40px; }
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
.rtt-pick { list-style: none; margin: 0 0 var(--space-3); padding: 0; display: flex; flex-direction: column; gap: var(--space-2); }
.rtt-pick__btn {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: var(--space-3);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface2);
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.rtt-pick__btn:hover { border-color: var(--accent); }
.rtt-pick__btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.rtt-pick__name { font-weight: 600; }
.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
</style>
