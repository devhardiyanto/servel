<script setup lang="ts">
// Projects (v3): daftar di kiri, detail project terpilih di kanan.
import { computed, ref, watch } from 'vue'
import ProjectsPanel from '@/components/ProjectsPanel.vue'
import ProjectDetail from '@/components/ProjectDetail.vue'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime } from '@/composables/useProjectRuntime'

const { config } = useConfig()
const { stateOf } = useProjectRuntime()

const selectedId = ref<string | null>(null)
const selected = computed(() => config.value.projects.find((p) => p.id === selectedId.value) ?? null)

// Pilih project pertama yang punya folder; lepas pilihan bila project dihapus.
watch(
  () => config.value.projects.map((p) => p.id).join(','),
  () => {
    if (selected.value) return
    selectedId.value = config.value.projects.find((p) => p.path)?.id ?? null
  },
  { immediate: true },
)
</script>

<template>
  <div class="pv">
    <div class="pv-list">
      <ProjectsPanel :selected-id="selectedId" @select="selectedId = $event" />
    </div>
    <aside class="pv-detail" aria-label="Project details">
      <template v-if="selected">
        <div class="pv-head">
          <div class="pv-name">{{ selected.name }}</div>
          <div class="pv-meta">
            <span v-if="selected.kind">{{ selected.kind }}</span>
            <span v-if="(selected.port ?? 0) > 0">localhost:{{ selected.port }}</span>
            <span>{{ stateOf(selected.id) }}</span>
          </div>
          <div class="pv-path" :title="selected.path">{{ selected.path || 'Folder not chosen yet' }}</div>
        </div>
        <ProjectDetail v-if="selected.path" :key="selected.id" :project="selected" />
        <p v-else class="pv-empty">Choose the folder first to see versions, services and output.</p>
      </template>
      <p v-else class="pv-empty">Select a project to see its versions, services and output.</p>
    </aside>
  </div>
</template>

<style scoped>
.pv {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 340px;
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
.pv-head { display: flex; flex-direction: column; gap: var(--space-1); }
.pv-name { font-size: var(--text-lg); font-weight: 600; color: var(--text); }
.pv-meta { display: flex; flex-wrap: wrap; gap: var(--space-3); font-size: var(--text-xs); color: var(--muted); font-family: var(--font-mono); }
.pv-path { font-size: var(--text-xs); color: var(--faint); font-family: var(--font-mono); overflow-wrap: anywhere; }
.pv-empty { margin: 0; font-size: var(--text-sm); color: var(--faint); }

@media (max-width: 1000px) {
  .pv { grid-template-columns: 1fr; grid-template-rows: auto auto; overflow-y: auto; }
  .pv-list, .pv-detail { overflow: visible; }
  .pv-detail { border-left: 0; border-top: 1px solid var(--border); }
}
</style>
