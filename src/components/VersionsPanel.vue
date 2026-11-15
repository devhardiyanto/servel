<script setup lang="ts">
// Versi PHP/Node per konteks (Global atau project) — pindah dari Dashboard ke
// halaman Runtimes di v3.
import { computed, inject, onMounted } from 'vue'
import UiButton from '@/components/ui/UiButton.vue'
import { usePhp } from '@/composables/usePhp'
import { useNode } from '@/composables/useNode'
import { useVersionContext } from '@/composables/useVersionContext'
import { useConfig } from '@/composables/useConfig'
import { useServices } from '@/composables/useServices'
import { SetViewKey } from '@/types/navigation'

const setView = inject(SetViewKey)!
const { applyProfile } = useServices()

const {
  versions: phpVersions,
  active: phpActive,
  switching: phpSwitching,
  switchTo: switchPhp,
  load: loadPhp,
} = usePhp()

const {
  versions: nodeVersions,
  active: nodeActive,
  switching: nodeSwitching,
  switchTo: switchNode,
  load: loadNode,
} = useNode()

const versionCtx = useVersionContext()
const {
  ctx: miseCtx,
  isMise,
  scopeRequest,
  untrustedPath,
  lastError: versionError,
  busy: versionBusy,
} = versionCtx
const projects = computed(() => useConfig().config.value.projects)
const activeProjectId = computed(() => useConfig().config.value.activeProjectId)
const phpSource = computed(() => versionCtx.sourceLabel('php'))
const nodeSource = computed(() => versionCtx.sourceLabel('node'))

const phpAccent = '#7cc7e8'
const nodeAccent = '#7fd6a4'

async function reloadVersions(): Promise<void> {
  await versionCtx.refresh()
  await Promise.all([loadPhp(), loadNode()])
}

// Memilih project juga menerapkan profil service-nya (bila ditautkan).
async function onContextChange(id: string): Promise<void> {
  await useConfig().setActiveProject(id || null)
  const project = projects.value.find((p) => p.id === id)
  if (project?.profileId) applyProfile(project.profileId)
  await reloadVersions()
}

async function resolveScope(scope: 'project' | 'global'): Promise<void> {
  const req = scopeRequest.value
  scopeRequest.value = null
  if (!req) return
  if (req.tool === 'php') await switchPhp(req.version, scope)
  else await switchNode(req.version, scope)
}

async function trustConfig(): Promise<void> {
  if (!untrustedPath.value) return
  await versionCtx.trust(untrustedPath.value)
  await Promise.all([loadPhp(), loadNode()])
}

function splitVersion(raw: string | null): { major: string; patch: string } {
  if (!raw) return { major: '—', patch: '' }
  const parts = raw.split('.')
  if (parts.length >= 3) {
    return { major: parts.slice(0, 2).join('.'), patch: '.' + parts.slice(2).join('.') }
  }
  return { major: raw, patch: '' }
}

const phpSplit = computed(() => splitVersion(phpActive.value))
const nodeSplit = computed(() => {
  if (!nodeActive.value) return { major: '—', patch: '' }
  const parts = nodeActive.value.split('.')
  if (parts.length >= 2) {
    return { major: parts[0], patch: '.' + parts.slice(1).join('.') }
  }
  return { major: nodeActive.value, patch: '' }
})

function step(list: { version: string }[], active: string | null, busy: boolean, dir: number, apply: (v: string) => void): void {
  if (busy || list.length === 0) return
  const idx = list.findIndex((v) => v.version === active)
  const current = idx === -1 ? 0 : idx
  const next = Math.min(list.length - 1, Math.max(0, current + dir))
  if (list[next].version !== active) apply(list[next].version)
}
function phpVersionStep(dir: number): void {
  step(phpVersions.value, phpActive.value, phpSwitching.value, dir, (v) => void switchPhp(v))
}
function nodeVersionStep(dir: number): void {
  step(nodeVersions.value, nodeActive.value, nodeSwitching.value, dir, (v) => void switchNode(v))
}
function phpIdx(): number {
  return phpVersions.value.findIndex((v) => v.version === phpActive.value)
}
function nodeIdx(): number {
  return nodeVersions.value.findIndex((v) => v.version === nodeActive.value)
}

onMounted(() => {
  void versionCtx.refresh()
})
</script>

<template>
  <div class="vp">
    <div v-if="untrustedPath" class="ctx-banner ctx-banner--warn">
      <div class="ctx-banner__text">
        <strong>mise does not trust this config yet</strong>
        <span class="ctx-banner__path">{{ untrustedPath }}</span>
      </div>
      <UiButton variant="accent" :disabled="versionBusy" @click="trustConfig">Trust</UiButton>
    </div>

    <div v-if="versionError" class="ctx-banner ctx-banner--error">
      <div class="ctx-banner__text">
        <strong>Version switch failed</strong>
        <span class="ctx-banner__path">{{ versionError }}</span>
      </div>
      <UiButton variant="quiet" @click="versionError = null">Dismiss</UiButton>
    </div>

    <div v-if="isMise" class="ctx-bar">
      <span class="ctx-bar__label">Versions for</span>
      <select
        class="ctx-select"
        :value="activeProjectId ?? ''"
        @change="onContextChange(($event.target as HTMLSelectElement).value)"
      >
        <option value="">Global</option>
        <option v-for="p in projects.filter((x) => x.path)" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
      <span v-if="miseCtx?.project" class="ctx-bar__path" :title="miseCtx.project.path">
        {{ miseCtx.project.path }}<template v-if="miseCtx.projectMissing"> — folder not found</template>
      </span>
      <span class="ctx-bar__spacer"></span>
      <span class="ctx-bar__hint">mise {{ miseCtx?.miseVersion ?? '' }}</span>
      <UiButton variant="quiet" @click="setView('projects')">Manage projects</UiButton>
    </div>

    <section class="control-strip">
      <div class="control-block" :style="{ borderColor: phpAccent + '40' }">
        <div class="cb-label">
          PHP Version
          <span class="cb-live" :style="{ color: phpAccent }">
            <span class="cb-live-dot" :style="{ background: phpAccent }"></span>
            active
          </span>
        </div>



        <div class="ver-row">
          <span class="ver-tag" :style="{ color: phpAccent }">{{ phpSplit.major }}</span>
          <span class="ver-patch">{{ phpSplit.patch }}</span>
          <span class="ver-spacer"></span>
          <button class="ver-arrow" :disabled="phpSwitching || phpIdx() <= 0" @click="phpVersionStep(-1)">&#8249;</button>
          <button class="ver-arrow" :disabled="phpSwitching || phpIdx() >= phpVersions.length - 1" @click="phpVersionStep(1)">&#8250;</button>
        </div>
        <div class="ver-chip-list">
          <button
            v-for="item in phpVersions"
            :key="item.version"
            class="ver-chip"
            :class="{ 'ver-chip--active': item.version === phpActive }"
            :style="item.version === phpActive
              ? { background: phpAccent + '14', color: phpAccent, borderColor: phpAccent + '40' }
              : {}"
            :disabled="phpSwitching"
            @click="item.version !== phpActive && switchPhp(item.version)"
          >{{ item.version }}</button>
          <span v-if="phpVersions.length === 0" class="ver-empty">no versions installed</span>
        </div>
        <div v-if="phpSource" class="ver-source" :title="phpSource">from {{ phpSource }}</div>
      </div>

      <div class="control-block" :style="{ borderColor: nodeAccent + '40' }">
        <div class="cb-label">
          Node Version
          <span class="cb-live" :style="{ color: nodeAccent }">
            <span class="cb-live-dot" :style="{ background: nodeAccent }"></span>
            active
          </span>
        </div>


        <div class="ver-row">
          <span class="ver-tag" :style="{ color: nodeAccent }">{{ nodeSplit.major }}</span>
          <span class="ver-patch">{{ nodeSplit.patch }}</span>
          <span class="ver-spacer"></span>
          <button class="ver-arrow" :disabled="nodeSwitching || nodeIdx() <= 0" @click="nodeVersionStep(-1)">&#8249;</button>
          <button class="ver-arrow" :disabled="nodeSwitching || nodeIdx() >= nodeVersions.length - 1" @click="nodeVersionStep(1)">&#8250;</button>
        </div>
        <div class="ver-chip-list">
          <button
            v-for="item in nodeVersions"
            :key="item.version"
            class="ver-chip"
            :class="{ 'ver-chip--active': item.version === nodeActive }"
            :style="item.version === nodeActive
              ? { background: nodeAccent + '14', color: nodeAccent, borderColor: nodeAccent + '40' }
              : {}"
            :disabled="nodeSwitching"
            @click="item.version !== nodeActive && switchNode(item.version)"
          >{{ item.version }}</button>
          <span v-if="nodeVersions.length === 0" class="ver-empty">no versions installed</span>
        </div>
        <div v-if="nodeSource" class="ver-source" :title="nodeSource">from {{ nodeSource }}</div>
      </div>
    </section>

    <div v-if="scopeRequest" class="scope-overlay" @click.self="scopeRequest = null">
        <div class="scope-dialog" role="dialog" aria-modal="true">
          <div class="scope-dialog__title">
            Use {{ scopeRequest.tool === 'php' ? 'PHP' : 'Node' }} {{ scopeRequest.version }} for {{ miseCtx?.project?.name }}?
          </div>
          <p class="scope-dialog__body">
            This project currently follows your global {{ scopeRequest.tool === 'php' ? 'PHP' : 'Node' }}
            version<template v-if="scopeRequest.globalVersion"> ({{ scopeRequest.globalVersion }})</template>.
          </p>
          <button class="scope-option scope-option--primary" @click="resolveScope('project')">
            <span>Pin in this project</span>
            <small>writes {{ scopeRequest.projectFile }}</small>
          </button>
          <button class="scope-option" @click="resolveScope('global')">
            <span>Change global version</span>
            <small>affects every project without its own pin</small>
          </button>
          <UiButton variant="quiet" class="scope-cancel" @click="scopeRequest = null">Cancel</UiButton>
        </div>
      </div>
  </div>
</template>

<style scoped>
.vp { display: flex; flex-direction: column; gap: var(--space-3); }
.control-strip {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: var(--space-4);
  flex-shrink: 0;
}
.control-block {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--space-4) var(--space-5);
  box-shadow: var(--shadow-md);
  transition: border-color 0.2s var(--ease);
}
.cb-label {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
  margin-bottom: var(--space-3);
}
.cb-live {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  font-weight: 400;
  letter-spacing: 0.04em;
}
.cb-live-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  animation: pulse 2s infinite;
}
.ver-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}
.ver-tag {
  font-family: var(--font-mono);
  font-size: var(--text-xl);
  font-weight: 400;
  line-height: 1;
}
.ver-patch {
  font-family: var(--font-mono);
  font-size: var(--text-md);
  color: var(--faint);
}
.ver-spacer {
  flex: 1;
}
.ver-arrow {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius-xs);
  color: var(--text);
  font-size: var(--text-lg);
  cursor: pointer;
  padding: 0;
  line-height: 1;
  transition: background 0.1s;
}
.ver-arrow:hover:not(:disabled) {
  background: var(--dim);
}
.ver-arrow:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.ver-chip-list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}
.ver-chip {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  padding: 3px 10px;
  border-radius: var(--radius-xs);
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--muted);
  cursor: pointer;
  transition: background 0.1s, color 0.1s, border-color 0.1s;
}
.ver-chip:hover:not(:disabled):not(.ver-chip--active) {
  background: var(--dim);
  color: var(--text);
}
.ver-chip--active {
  font-weight: 600;
}
.ver-chip:disabled {
  cursor: not-allowed;
}
.ver-empty {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--faint);
}
/* --- Konteks versi (mise) --- */
.ctx-bar {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  background: var(--surface);
  border: 1px solid var(--border);
  font-size: var(--text-xs);
  flex-shrink: 0;
}
.ctx-bar__label { color: var(--muted); }
.ctx-bar__path {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.ctx-bar__spacer { flex: 1; }
.ctx-bar__hint { color: var(--faint); font-size: var(--text-xs); white-space: nowrap; }
.ctx-select {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface2);
  color: var(--text);
}
.ctx-banner {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  flex-shrink: 0;
}
.ctx-banner--warn {
  background: color-mix(in srgb, var(--amber) 9%, transparent);
  border: 1px solid color-mix(in srgb, var(--amber) 30%, transparent);
}
.ctx-banner--error {
  background: color-mix(in srgb, var(--red) 9%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 30%, transparent);
}
.ctx-banner__text { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; font-size: var(--text-xs); }
.ctx-banner--warn strong { color: var(--amber); }
.ctx-banner--error strong { color: var(--red); }
.ctx-banner__path {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ver-source {
  margin-top: var(--space-2);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.scope-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
  z-index: 50;
}
.scope-dialog {
  width: min(460px, calc(100vw - 32px));
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-5);
  border-radius: var(--radius-lg);
  background: var(--surface);
  border: 1px solid var(--border-strong);
  box-shadow: var(--shadow-md);
}
.scope-dialog__title { font-size: var(--text-md); font-weight: 600; color: var(--text); }
.scope-dialog__body { font-size: var(--text-xs); color: var(--muted); line-height: 1.55; }
.scope-option {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background: var(--surface2);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  text-align: left;
  cursor: pointer;
}
.scope-option small {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--faint);
  word-break: break-all;
}
.scope-option--primary {
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  background: color-mix(in srgb, var(--accent) 10%, var(--surface2));
}
.scope-cancel { align-self: flex-end; }
.control-strip { grid-template-columns: 1fr 1fr; }
</style>
