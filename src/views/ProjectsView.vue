<script setup lang="ts">
// Projects: daftar di kiri, detail project terpilih di kanan.
import { computed, inject, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { appConfigDir, join } from '@tauri-apps/api/path'
import ProjectsPanel from '@/components/ProjectsPanel.vue'
import ProjectDetail from '@/components/ProjectDetail.vue'
import ProjectDialogs from '@/components/ProjectDialogs.vue'
import AddProjectModal from '@/components/AddProjectModal.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiIcon from '@/components/ui/UiIcon.vue'
import { useProjectActions } from '@/composables/useProjectActions'
import { useServices } from '@/composables/useServices'
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime } from '@/composables/useProjectRuntime'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'
import PanelSplitter from '@/components/ui/PanelSplitter.vue'
import { PROJECTS_PANEL, usePanelWidth } from '@/composables/usePanelWidth'
import { useUnsavedGuard } from '@/composables/useUnsavedGuard'

const { config, loaded, loadError, load } = useConfig()

const retrying = ref(false)
/** Where config.json lives, shown with the read error so it can be fixed by hand. */
const configDir = ref('')
const configFile = ref('')
watch(loadError, async (e) => {
  if (!e || configFile.value) return
  try {
    configDir.value = await appConfigDir()
    configFile.value = await join(configDir.value, 'config.json')
  } catch {
    configFile.value = ''
  }
})
async function openConfigDir(): Promise<void> {
  if (configDir.value) await invoke('project_open_in', { target: 'folder', path: configDir.value }).catch(() => {})
}

async function retry(): Promise<void> {
  retrying.value = true
  try {
    await load()
  } catch {
    /* pesan ada di loadError */
  } finally {
    retrying.value = false
  }
}
useProjectRuntime()
const actions = useProjectActions()
const guard = useUnsavedGuard()
const setView = inject(SetViewKey, () => {})
const query = ref('')

// Strip service di bawah daftar: service yang sedang berjalan.
const services = useServices()
const runningServices = computed(() =>
  services.definitions.value.filter((d) => services.uiState.value[d.id]?.status === 'running').map((d) => d.name),
)

// Panel detail: lebar bisa diseret dan panel bisa ditutup.
const PANEL = PROJECTS_PANEL
const panel = usePanelWidth('projects', PANEL)

const selectedId = ref<string | null>(null)
/**
 * Klik kartu membuka panel yang tertutup; klik kartu yang sedang terbuka
 * menutup panel supaya daftar melebar. Pilihan tetap diingat untuk tombol ‹.
 */
async function choose(id: string): Promise<void> {
  if (selectedId.value === id && !panel.collapsed.value) {
    await closePanel()
    return
  }
  if (selectedId.value !== id && !(await guard.confirmLeave())) return
  selectedId.value = id
  panel.collapsed.value = false
}
/** Menutup panel membuang isian Environment yang belum disimpan: tanya dulu. */
async function closePanel(): Promise<void> {
  if (await guard.confirmLeave()) panel.collapsed.value = true
}
/** Esc di daftar menutup panel; Esc di kolom isian tetap milik kolom itu. */
function onListEsc(e: KeyboardEvent): void {
  const t = e.target as HTMLElement | null
  if (t?.closest('input, textarea, select, [role="menu"]')) return
  if (!panel.collapsed.value && config.value.projects.length) void closePanel()
}
function newGroup(): void {
  actions.renamingGroupId.value = null
  actions.draftGroup.value = { projectId: null }
}
const selected = computed(() => config.value.projects.find((p) => p.id === selectedId.value) ?? null)

// Tanpa pilihan (awal, atau project terpilih dihapus): pilih project pertama.
watch(
  () => config.value.projects.map((p) => p.id).join(','),
  () => {
    if (selected.value) return
    selectedId.value = config.value.projects[0]?.id ?? null
  },
  { immediate: true },
)
// Permintaan dari layar lain (Logs → Open project).
watch(
  () => actions.selectRequest.value,
  async (id) => {
    if (!id) return
    actions.selectRequest.value = null
    if (id !== selectedId.value && !(await guard.confirmLeave())) return
    selectedId.value = id
  },
  { immediate: true },
)
watch(selectedId, () => (actions.detailTab.value = 'overview'))
</script>

<template>
  <div v-if="loadError" class="pv-state">
    <UiBanner tone="err" title="Couldn't load your projects">
      {{ loadError }} Kova did not change the file.
      <code v-if="configFile" class="pv-config-path">{{ configFile }}</code>
      <template #actions>
        <UiButton v-if="configDir" variant="quiet" @click="openConfigDir">Open folder</UiButton>
        <UiButton :loading="retrying" @click="retry">Try again</UiButton>
      </template>
    </UiBanner>
    <div class="pv-unavailable">
      <h2>Projects are unavailable</h2>
      <p>Services and runtimes still work.</p>
    </div>
  </div>
  <div v-else-if="!loaded" class="pv-state" aria-busy="true" aria-label="Loading projects">
    <div v-for="n in 3" :key="n" class="pv-skel">
      <SkeletonBox width="38%" height="14px" />
      <SkeletonBox width="62%" height="11px" />
      <SkeletonBox width="180px" height="26px" radius="6px" />
    </div>
  </div>
  <div v-else class="pv" :class="{ 'pv--empty': !config.projects.length }">
    <section class="pv-list" aria-label="Project list" @keydown.esc="onListEsc">
      <header class="pv-bar">
        <h1 class="pv-title">Projects <span v-if="config.projects.length" class="pv-count">{{ config.projects.length }}</span></h1>
        <div v-if="config.projects.length" class="pv-search">
          <UiIcon name="search" class="pv-search__i" />
          <UiInput
            v-model="query"
            type="search"
            placeholder="Search projects"
            aria-label="Search projects"
            @keydown.esc="query = ''"
          />
        </div>
        <span class="pv-sp"></span>
        <!-- Tanpa project, Add project ada di empty state; jangan dobel. -->
        <template v-if="config.projects.length">
          <UiButton size="md" title="New group" :disabled="!!actions.draftGroup.value" @click="newGroup"><UiIcon name="plus" /><span class="pv-newgroup-label">New group</span></UiButton>
          <UiButton variant="primary" size="md" title="Add project" @click="actions.adding.value = true"><UiIcon name="plus" /><span class="pv-add-label">Add project</span></UiButton>
        </template>
      </header>
      <ProjectsPanel :selected-id="selectedId" :query="query" :details-open="!panel.collapsed.value" @select="choose" @clear-search="query = ''" />
      <div v-if="config.projects.length" class="pv-strip">
        <span>{{ runningServices.length ? `${runningServices.join(' · ')} running` : 'No services running' }}</span>
        <UiButton variant="quiet" @click="setView('services')">Services</UiButton>
      </div>
    </section>
    <!-- Panel detail hanya berguna bila ada project yang bisa dipilih. -->
    <template v-if="config.projects.length">
      <template v-if="!panel.collapsed.value">
        <PanelSplitter
          v-model:width="panel.width.value"
          v-bind="{ min: PANEL.min, max: PANEL.max, defaultWidth: PANEL.default }"
          controls="pv-detail"
          label="Resize project details"
          @collapse="closePanel"
        />
        <aside id="pv-detail" class="pv-detail" aria-label="Project details" :style="{ width: `${panel.width.value}px` }">
          <ProjectDetail v-if="selected" :key="selected.id" :project="selected" />
          <p v-else class="pv-empty">Select a project to see its status, environment and output.</p>
        </aside>
      </template>
      <button
        v-else
        class="pv-reopen"
        aria-controls="pv-detail"
        aria-expanded="false"
        :title="selected ? `Show ${selected.name} details` : 'Show project details'"
        @click="panel.collapsed.value = false"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 4L6 8l4 4" /></svg>
        <span class="sr-only">{{ selected ? `Show ${selected.name} details` : 'Show project details' }}</span>
      </button>
    </template>
    <ProjectDialogs />
    <AddProjectModal @added="selectedId = $event" />
  </div>
</template>

<style scoped>
.pv-search { position: relative; flex: 0 1 320px; min-width: 140px; }
.pv-search :deep(.ui-field) { width: 100%; }
.pv-search__i { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--muted); pointer-events: none; z-index: 1; }
.pv-search :deep(input) { padding-left: 32px; }
.pv-sp { flex: 1; }

.pv-config-path { display: block; margin-top: var(--space-2); font: 400 var(--text-xs) var(--font-mono); color: var(--muted); word-break: break-all; }
.pv-unavailable { text-align: center; padding: var(--space-8) 0; }
.pv-unavailable h2 { margin: 0 0 var(--space-1); font-size: var(--text-md); font-weight: 600; color: var(--text); }
.pv-unavailable p { margin: 0; font-size: var(--text-sm); color: var(--faint); }
.pv-state { padding: var(--space-6) var(--space-8); display: flex; flex-direction: column; gap: var(--space-3); max-width: 720px; }
.pv-skel {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-4);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--surface);
}

.pv {
  display: flex;
  height: 100%;
  min-height: 0;
}
.pv-list {
  flex: 1;
  min-width: 0;
  overflow-x: hidden;
  overflow-y: auto;
  padding: 18px 20px 16px;
  /* Header dan kartu menyesuaikan lebar daftar, bukan lebar jendela:
     panel detail yang terbuka mempersempit daftar. */
  container: pv-list / inline-size;
}
.pv-detail {
  flex: none;
  max-width: 60%;
  overflow-y: auto;
  padding: 16px;
  background: var(--surface);
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}
.pv-list { display: flex; flex-direction: column; gap: var(--space-4); }
.pv-bar { display: flex; align-items: center; gap: var(--space-3); min-width: 0; }
.pv-bar :deep(.ui-btn) { flex: none; }
/* Daftar sempit: New group tinggal ikon (aria-label tetap "New group"). */
@container pv-list (max-width: 600px) {
  .pv-newgroup-label { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
  .pv-search { flex-basis: 200px; min-width: 96px; }
}
/* Daftar sangat sempit (jendela 900 px dengan panel terbuka): Add project juga ikon. */
@container pv-list (max-width: 420px) {
  .pv-bar { flex-wrap: wrap; row-gap: var(--space-2); }
  .pv-search { order: 5; flex: 1 1 100%; }
  .pv-add-label { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
}
.pv-title { margin: 0; font-size: var(--text-xl); white-space: nowrap; font-weight: 600; color: var(--text); }
.pv-count { font: 400 var(--text-xs) var(--font-mono); color: var(--faint); margin-left: var(--space-1); }
.pv-strip {
  margin-top: auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  color: var(--muted);
}
.pv-empty { margin: 0; font-size: var(--text-sm); color: var(--faint); }
/* Panel tertutup: strip tipis di tepi kanan untuk membukanya lagi. */
.pv-reopen {
  flex: none;
  width: 28px;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: var(--space-6);
  border: 0;
  border-left: 1px solid var(--border);
  background: var(--bg-sunken);
  color: var(--muted);
  cursor: pointer;
}
.pv-reopen:hover { color: var(--text); background: var(--surface); }
.pv-reopen:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.pv-reopen svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }

/* Jendela sempit (900–1000 px): panel tetap di kanan supaya klik kartu
   langsung terlihat hasilnya; panel dibatasi separuh lebar layar. */
@media (max-width: 1000px) {
  .pv-detail { max-width: 52%; }
  .pv-list { padding: 16px 14px 12px; }
}
</style>
