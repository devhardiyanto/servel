<script setup lang="ts">
// Projects: daftar di kiri, detail project terpilih di kanan.
import { computed, inject, ref, watch } from 'vue'
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

const { config, loaded, loadError, load } = useConfig()

const retrying = ref(false)
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
function choose(id: string): void {
  if (selectedId.value === id && !panel.collapsed.value) {
    panel.collapsed.value = true
    return
  }
  selectedId.value = id
  panel.collapsed.value = false
}
/** Esc di daftar menutup panel; Esc di kolom isian tetap milik kolom itu. */
function onListEsc(e: KeyboardEvent): void {
  const t = e.target as HTMLElement | null
  if (t?.closest('input, textarea, select, [role="menu"]')) return
  if (!panel.collapsed.value && config.value.projects.length) panel.collapsed.value = true
}
function newGroup(): void {
  actions.renamingGroupId.value = null
  actions.draftGroup.value = { projectId: null }
}
const selected = computed(() => config.value.projects.find((p) => p.id === selectedId.value) ?? null)

// Pilih project pertama yang punya folder; lepas pilihan bila project dihapus.
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
  (id) => {
    if (!id) return
    selectedId.value = id
    actions.selectRequest.value = null
  },
  { immediate: true },
)
// Ganti project = mulai dari tab Overview.
watch(selectedId, () => (actions.detailTab.value = 'overview'))
</script>

<template>
  <div v-if="loadError" class="pv-state">
    <UiBanner tone="err" title="Kova could not read its settings">
      {{ loadError }}
      <template #actions><UiButton :loading="retrying" @click="retry">Try again</UiButton></template>
    </UiBanner>
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
          <UiButton size="md" :disabled="!!actions.draftGroup.value" @click="newGroup"><UiIcon name="plus" />New group</UiButton>
          <UiButton variant="primary" size="md" @click="actions.adding.value = true"><UiIcon name="plus" />Add project</UiButton>
        </template>
      </header>
      <ProjectsPanel :selected-id="selectedId" :query="query" :details-open="!panel.collapsed.value" @select="choose" />
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
          @collapse="panel.collapsed.value = true"
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
  overflow-y: auto;
  padding: 18px 20px 16px;
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
.pv-bar { display: flex; align-items: center; gap: var(--space-3); }
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

@media (max-width: 1000px) {
  .pv { flex-direction: column; overflow-y: auto; }
  .pv-list, .pv-detail { overflow: visible; }
  .pv-detail { width: auto !important; max-width: none; border-left: 0; border-top: 1px solid var(--border); }
  .pv > .ps { display: none; }
  .pv-reopen { width: auto; padding: var(--space-2); border-left: 0; border-top: 1px solid var(--border); }
}
</style>
