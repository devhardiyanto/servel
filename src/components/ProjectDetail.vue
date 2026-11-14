<script setup lang="ts">
// Panel detail project (v2.10): versi runtime dari mise, service dari
// profil, output terakhir dev server, dan pintasan buka folder.
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiButton from '@/components/ui/UiButton.vue'
import LogTail from '@/components/LogTail.vue'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import type { Project, ToolCurrent } from '@/types/mise'

const props = defineProps<{ project: Project }>()

const { config } = useConfig()
const { lines } = useLogs()

const tools = ref<ToolCurrent[] | null>(null)
const toolsError = ref('')
const openError = ref('')

const profile = computed(() =>
  props.project.profileId ? config.value.profiles.find((p) => p.id === props.project.profileId) : undefined,
)

// Dev server memakai nama project sebagai sumber log.
const output = computed(() => lines.value.filter((l) => l.src === props.project.name).slice(-20))

function sourceLabel(t: ToolCurrent): string {
  if (t.scope === 'global') return 'global'
  if (!t.source) return ''
  return t.source.split(/[\\/]/).pop() ?? ''
}

async function loadTools(): Promise<void> {
  toolsError.value = ''
  try {
    tools.value = await invoke<ToolCurrent[]>('mise_project_tools', { path: props.project.path })
  } catch (err) {
    tools.value = []
    toolsError.value = String(err)
  }
}

async function openIn(target: 'folder' | 'terminal' | 'vscode'): Promise<void> {
  openError.value = ''
  try {
    await invoke('project_open_in', { target, path: props.project.path })
  } catch (err) {
    openError.value = String(err)
  }
}

onMounted(loadTools)
</script>

<template>
  <div class="pd">
    <div class="pd-actions">
      <UiButton variant="quiet" @click="openIn('folder')">Open folder</UiButton>
      <UiButton variant="quiet" @click="openIn('terminal')">Terminal here</UiButton>
      <UiButton variant="quiet" @click="openIn('vscode')">Open in VS Code</UiButton>
    </div>
    <p v-if="openError" class="pd-error">{{ openError }}</p>

    <dl class="pd-grid">
      <dt>Runtime</dt>
      <dd>
        <span v-if="tools === null" class="pd-dim">Checking…</span>
        <span v-else-if="toolsError" class="pd-dim">{{ toolsError }}</span>
        <span v-else-if="tools.length === 0" class="pd-dim">No versions set by mise for this folder</span>
        <template v-else>
          <span v-for="t in tools" :key="t.tool" class="pd-tool">
            {{ t.tool }} <b>{{ t.version }}</b>
            <span v-if="!t.installed" class="pd-warn">not installed</span>
            <span v-if="sourceLabel(t)" class="pd-dim">· {{ sourceLabel(t) }}</span>
          </span>
        </template>
      </dd>

      <dt>Services</dt>
      <dd>
        <template v-if="profile">
          {{ profile.name }}:
          <span class="pd-mono">{{ profile.serviceIds.join(', ') || 'none' }}</span>
          <span class="pd-dim">· {{ project.startServices ? 'started with the project' : 'not started with the project' }}</span>
        </template>
        <span v-else class="pd-dim">No service profile. Pick one in Edit.</span>
      </dd>
    </dl>

    <div class="pd-out-label">Last output</div>
    <LogTail v-if="output.length" :lines="output" max-height="160px" />
    <p v-else class="pd-dim pd-empty">No output yet in this session.</p>
  </div>
</template>

<style scoped>
.pd {
  margin-top: var(--space-3);
  padding-top: var(--space-3);
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  font-size: var(--text-xs);
}
.pd-actions { display: flex; gap: var(--space-2); flex-wrap: wrap; }
.pd-grid {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-1) var(--space-4);
  margin: var(--space-1) 0;
}
.pd-grid dt { color: var(--faint); }
.pd-grid dd { margin: 0; color: var(--text); display: flex; flex-wrap: wrap; gap: var(--space-1) var(--space-3); }
.pd-tool { display: inline-flex; align-items: baseline; gap: var(--space-1); }
.pd-tool b { font-family: var(--font-mono); font-weight: 500; }
.pd-mono { font-family: var(--font-mono); }
.pd-dim { color: var(--faint); }
.pd-warn { color: var(--amber); }
.pd-error { margin: 0; color: var(--red); }
.pd-out-label { color: var(--faint); text-transform: uppercase; letter-spacing: 0.06em; }
.pd-empty { margin: 0; }
</style>
