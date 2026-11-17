<script setup lang="ts">
// Projects (v3): daftar di kiri, detail project terpilih di kanan.
import { computed, inject, ref, watch } from 'vue'
import ProjectsPanel from '@/components/ProjectsPanel.vue'
import ProjectDetail from '@/components/ProjectDetail.vue'
import ProjectDialogs from '@/components/ProjectDialogs.vue'
import AddProjectModal from '@/components/AddProjectModal.vue'
import UiInput from '@/components/ui/UiInput.vue'
import { useProjectActions } from '@/composables/useProjectActions'
import { useServices } from '@/composables/useServices'
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime } from '@/composables/useProjectRuntime'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiButton from '@/components/ui/UiButton.vue'

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

const selectedId = ref<string | null>(null)
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
  <div v-else class="pv">
    <section class="pv-list" aria-label="Project list">
      <header class="pv-bar">
        <h1 class="pv-title">Projects <span class="pv-count">{{ config.projects.length }}</span></h1>
        <UiInput
          v-if="config.projects.length > 3"
          v-model="query"
          type="search"
          width="220px"
          placeholder="Search projects"
          aria-label="Search projects"
          @keydown.esc="query = ''"
        />
        <UiButton variant="primary" @click="actions.adding.value = true">Add project</UiButton>
      </header>
      <ProjectsPanel :selected-id="selectedId" :query="query" @select="selectedId = $event" />
      <div v-if="config.projects.length" class="pv-strip">
        <span>{{ runningServices.length ? `${runningServices.join(' · ')} running` : 'No services running' }}</span>
        <UiButton variant="quiet" @click="setView('services')">Services</UiButton>
      </div>
    </section>
    <aside class="pv-detail" aria-label="Project details">
      <ProjectDetail v-if="selected" :key="selected.id" :project="selected" />
      <p v-else class="pv-empty">Select a project to see its status, environment and output.</p>
    </aside>
    <ProjectDialogs />
    <AddProjectModal @added="selectedId = $event" />
  </div>
</template>

<style scoped>
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
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(360px, 420px);
  height: 100%;
  min-height: 0;
}
.pv-list {
  overflow-y: auto;
  padding: var(--space-6) var(--space-6) var(--space-6) var(--space-8);
}
.pv-detail {
  overflow-y: auto;
  padding: var(--space-6) var(--space-5);
  background: var(--bg-sunken);
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}
.pv-list { display: flex; flex-direction: column; gap: var(--space-4); }
.pv-bar { display: flex; align-items: center; gap: var(--space-3); }
.pv-title { margin: 0 auto 0 0; font-size: var(--text-lg); font-weight: 600; color: var(--text); }
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

@media (max-width: 1000px) {
  .pv { grid-template-columns: 1fr; grid-template-rows: auto auto; overflow-y: auto; }
  .pv-list, .pv-detail { overflow: visible; }
  .pv-detail { border-left: 0; border-top: 1px solid var(--border); }
}
</style>
