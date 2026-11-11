<script setup lang="ts">
import { onMounted, onUnmounted, computed, inject, ref, watch } from 'vue'
import { usePhp } from '@/composables/usePhp'
import { usePhpHookStatus } from '@/composables/usePhpHookStatus'
import { useNode } from '@/composables/useNode'
import { useVersionContext } from '@/composables/useVersionContext'
import { useLogs } from '@/composables/useLogs'
import { useServices } from '@/composables/useServices'
import { useConfig } from '@/composables/useConfig'
import { useUptime } from '@/composables/useUptime'
import { useEngineStatus } from '@/composables/useEngineStatus'
import { SetViewKey } from '@/types/navigation'
import LogTail from '@/components/LogTail.vue'
import ServiceRow from '@/components/ServiceRow.vue'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'

const SKELETON_ROW_COUNT = 9

const {
  versions: phpVersions,
  active: phpActive,
  switching: phpSwitching,
  switchTo: switchPhp,
  install: installPhp,
  load: loadPhp,
  suggested: phpSuggested,
  suggestedNeedsInstall: phpSuggestedNeedsInstall,
  dismissSuggestion: dismissPhpSuggestion,
} = usePhp()

const {
  versions: nodeVersions,
  active: nodeActive,
  switching: nodeSwitching,
  switchTo: switchNode,
  install: installNode,
  load: loadNode,
  suggested: nodeSuggested,
  suggestedNeedsInstall: nodeSuggestedNeedsInstall,
  dismissSuggestion: dismissNodeSuggestion,
} = useNode()

const setView = inject(SetViewKey)!

// --- Konteks versi (mise): Global atau project tertentu -------------------
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

async function reloadVersions(): Promise<void> {
  await versionCtx.refresh()
  await Promise.all([loadPhp(), loadNode()])
}

// v1.9: memilih project juga menerapkan profil service-nya (bila ditautkan).
// Hanya mengubah selection; container yang sedang jalan tidak di-stop.
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
const { status: phpHookStatus, refresh: refreshPhpHookStatus } = usePhpHookStatus()

const phpHookCommand = 'phpvm hook install'
const phpHookCopied = ref(false)
async function copyPhpHookCommand(): Promise<void> {
  try {
    await navigator.clipboard.writeText(phpHookCommand)
    phpHookCopied.value = true
    setTimeout(() => { phpHookCopied.value = false }, 1500)
  } catch {
    // clipboard tidak available — silent
  }
}

const { lines: logLines } = useLogs()
const { formatted: uptimeFormatted } = useUptime()
const {
  engineStatus,
  engine,
  engineLabel,
  wslcAvailable,
  lastError: engineLastError,
  refresh: refreshEngine,
} = useEngineStatus()

const refreshingEngine = ref(false)
async function handleEngineRefresh(): Promise<void> {
  if (refreshingEngine.value) return
  refreshingEngine.value = true
  try {
    await refreshEngine()
  } finally {
    refreshingEngine.value = false
  }
}

// Docker mati tapi WSLC tersedia → tawarkan pindah engine satu klik.
const canSwitchToWslc = computed(() => engine.value === 'docker' && wslcAvailable.value)
async function switchToWslc(): Promise<void> {
  if (refreshingEngine.value) return
  refreshingEngine.value = true
  try {
    await useConfig().setContainerRuntime('wslc')
    await refreshEngine()
  } finally {
    refreshingEngine.value = false
  }
}

const phpAccent = '#7cc7e8'
const nodeAccent = '#7fd6a4'
const RUNTIME_RAM_BASELINE = 154

const {
  uiState,
  coreServices,
  additionalServices,
  runningCount,
  runningIds,
  hasStarting,
  hasStopping,
  definitions,
  serviceError,
  portConflicts,
  load: loadServices,
  toggle,
  setSelectedIds,
  reconcileSelectedWithRunning,
  applyProfile,
  start,
  stopAll,
  dismissPortConflicts,
  startIgnoringConflicts,
} = useServices()

const profiles = computed(() => useConfig().config.value.profiles)
const activeProfileId = computed(() => useConfig().config.value.activeProfileId)

function onProfileChange(e: Event): void {
  const id = (e.target as HTMLSelectElement).value
  if (id) applyProfile(id)
}

const totalRamWithBaseline = computed(() => {
  const runningRam = runningIds.value.reduce((acc, id) => {
    const ui = uiState.value[id]
    if (ui?.memMb != null) return acc + ui.memMb
    const def = definitions.value.find((d) => d.id === id)
    return acc + (def?.ramEstimateMb ?? 0)
  }, 0)
  return runningRam + RUNTIME_RAM_BASELINE
})

const stoppedCount = computed(() => definitions.value.length - runningCount.value)

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

function phpVersionStep(dir: number): void {
  if (phpSwitching.value || phpVersions.value.length === 0) return
  const idx = phpVersions.value.findIndex((v) => v.version === phpActive.value)
  const current = idx === -1 ? 0 : idx
  const next = Math.min(phpVersions.value.length - 1, Math.max(0, current + dir))
  if (phpVersions.value[next].version !== phpActive.value) {
    switchPhp(phpVersions.value[next].version)
  }
}

function nodeVersionStep(dir: number): void {
  if (nodeSwitching.value || nodeVersions.value.length === 0) return
  const idx = nodeVersions.value.findIndex((v) => v.version === nodeActive.value)
  const current = idx === -1 ? 0 : idx
  const next = Math.min(nodeVersions.value.length - 1, Math.max(0, current + dir))
  if (nodeVersions.value[next].version !== nodeActive.value) {
    switchNode(nodeVersions.value[next].version)
  }
}

function phpIdx(): number {
  return phpVersions.value.findIndex((v) => v.version === phpActive.value)
}

function nodeIdx(): number {
  return nodeVersions.value.findIndex((v) => v.version === nodeActive.value)
}

const handleWindowFocus = (): void => {
  void refreshPhpHookStatus()
}

watch(phpActive, () => {
  void refreshPhpHookStatus()
})

onMounted(async () => {
  void versionCtx.refresh()
  await loadServices()
  void refreshPhpHookStatus()
  await useConfig().load()

  const cfg = useConfig().config.value

  // Restore selection UI dari config
  if (cfg.selectedServiceIds.length > 0) {
    setSelectedIds(cfg.selectedServiceIds)
  }

  // Rekonsiliasi: container yang auto-restart (restart: unless-stopped) setelah
  // reboot tampil running → switch harus ON walau tidak ada di saved config.
  // Dipanggil SETELAH config loaded + setSelectedIds; persist watch akan menulis
  // ulang selection dari running state (menyembuhkan config yang hilang).
  reconcileSelectedWithRunning()

  window.addEventListener('focus', handleWindowFocus)
})

onUnmounted(() => {
  window.removeEventListener('focus', handleWindowFocus)
})
</script>

<template>
  <div class="view view-dashboard">
    <header class="app-strip">
      <div class="app-strip__brand">
        <span class="app-strip__name">servel</span>
        <span class="app-strip__tag">local dev environment</span>
      </div>
      <div class="app-strip__right">
        <span class="app-strip__uptime">uptime {{ uptimeFormatted }}</span>
        <button class="app-strip__settings-btn" title="Settings" @click="setView('settings')">&#9881;</button>
      </div>
    </header>

    <div class="dashboard-content">
    <div v-if="engineStatus === 'down'" class="docker-down-banner">
      <span class="ddb-icon">&#9888;</span>
      <div class="ddb-text">
        <strong>{{ engineLabel }} is not responding</strong>
        <span class="ddb-sub">
          <template v-if="engine === 'wslc'">Run <code>wsl --update</code>, then Retry.</template>
          <template v-else-if="canSwitchToWslc">Start Docker Desktop, or switch to the built-in WSL containers.</template>
          <template v-else>Start Docker Desktop to use services.</template>
          <span v-if="engineLastError"> ({{ engineLastError }})</span>
        </span>
      </div>
      <button
        v-if="canSwitchToWslc"
        class="ddb-retry ddb-retry--primary"
        :disabled="refreshingEngine"
        @click="switchToWslc"
      >
        Use WSL containers
      </button>
      <button class="ddb-retry" :disabled="refreshingEngine" @click="handleEngineRefresh">
        {{ refreshingEngine ? '...' : 'Retry' }}
      </button>
    </div>

    <div v-if="portConflicts.length > 0" class="port-conflict-banner">
      <span class="pcb-icon">&#9888;</span>
      <div class="pcb-text">
        <strong>Konflik port terdeteksi</strong>
        <span class="pcb-sub">
          Port berikut sudah dipakai proses lain:
          <template v-for="(c, i) in portConflicts" :key="c.serviceId + ':' + c.port">
            <span class="pcb-port">{{ c.serviceName }} ({{ c.port }})</span><span v-if="i < portConflicts.length - 1">, </span>
          </template>
        </span>
      </div>
      <div class="pcb-actions">
        <button class="pcb-force" @click="startIgnoringConflicts">Tetap mulai</button>
        <button class="pcb-dismiss" @click="dismissPortConflicts">Tutup</button>
      </div>
    </div>

    <div v-if="untrustedPath" class="ctx-banner ctx-banner--warn">
      <div class="ctx-banner__text">
        <strong>mise does not trust this config yet</strong>
        <span class="ctx-banner__path">{{ untrustedPath }}</span>
      </div>
      <button class="ctx-btn ctx-btn--primary" :disabled="versionBusy" @click="trustConfig">Trust</button>
    </div>

    <div v-if="versionError" class="ctx-banner ctx-banner--error">
      <div class="ctx-banner__text">
        <strong>Version switch failed</strong>
        <span class="ctx-banner__path">{{ versionError }}</span>
      </div>
      <button class="ctx-btn" @click="versionError = null">Dismiss</button>
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
      <button class="ctx-btn" @click="setView('settings')">Manage projects</button>
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

        <div v-if="phpHookStatus === 'not_installed'" class="vp-hook-banner">
          <span class="vp-hook-text">
            Run <code class="vp-hook-cmd">phpvm hook install</code> di PowerShell untuk PATH integration.
          </span>
          <button class="vp-hook-copy" :title="phpHookCopied ? 'Copied' : 'Copy command'" @click="copyPhpHookCommand">
            {{ phpHookCopied ? '✓ Copied' : 'Copy' }}
          </button>
        </div>

        <div v-if="phpSuggested" class="vp-suggest-banner">
          <span class="vp-suggest-text">
            Project suggests PHP <strong>{{ phpSuggested.version }}</strong>
          </span>
          <div class="vp-suggest-actions">
            <button
              v-if="!phpSuggestedNeedsInstall"
              class="vp-suggest-btn"
              :disabled="phpSwitching"
              @click="switchPhp(phpSuggested.version)"
            >Switch</button>
            <button
              v-else
              class="vp-suggest-btn"
              :disabled="phpSwitching"
              @click="installPhp(phpSuggested.version)"
            >Install + Switch</button>
            <button class="vp-suggest-dismiss" @click="dismissPhpSuggestion">&times;</button>
          </div>
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

        <div v-if="nodeSuggested" class="vp-suggest-banner">
          <span class="vp-suggest-text">
            Project suggests Node <strong>{{ nodeSuggested.version }}</strong>
          </span>
          <div class="vp-suggest-actions">
            <button
              v-if="!nodeSuggestedNeedsInstall"
              class="vp-suggest-btn"
              :disabled="nodeSwitching"
              @click="switchNode(nodeSuggested.version)"
            >Switch</button>
            <button
              v-else
              class="vp-suggest-btn"
              :disabled="nodeSwitching"
              @click="installNode(nodeSuggested.version)"
            >Install + Switch</button>
            <button class="vp-suggest-dismiss" @click="dismissNodeSuggestion">&times;</button>
          </div>
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

      <div class="control-block control-block--stats">
        <div class="cb-label">Session Stats</div>
        <div class="stat-grid">
          <div class="stat-item">
            <div class="stat-val stat-val--green">{{ runningCount }}</div>
            <div class="stat-label">Running</div>
          </div>
          <div class="stat-item">
            <div class="stat-val stat-val--amber">{{ totalRamWithBaseline }}</div>
            <div class="stat-label">MB RAM</div>
          </div>
          <div class="stat-item">
            <div class="stat-val stat-val--dim stat-val--small">{{ stoppedCount }}</div>
            <div class="stat-label">Stopped</div>
          </div>
          <div class="stat-item">
            <div class="stat-val stat-val--dim stat-val--small">{{ definitions.length }}</div>
            <div class="stat-label">Total</div>
          </div>
        </div>
      </div>
    </section>

    <p v-if="serviceError" class="dash-error">
      {{ serviceError }}
    </p>

    <section class="services-wrap">
      <header class="sw-header">
        <span class="sw-title">Services</span>
        <span class="sw-counter">{{ runningCount }}/{{ definitions.length }}</span>
        <div v-if="profiles.length > 0" class="sw-profile">
          <span class="sw-profile__label">profil</span>
          <select
            class="sw-profile__select"
            :value="activeProfileId ?? ''"
            title="Ganti profil aktif"
            @change="onProfileChange"
          >
            <option v-for="p in profiles" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </div>
        <div class="sw-actions">
          <button
            class="sw-btn sw-btn--stop"
            :disabled="hasStopping || runningCount === 0"
            @click="stopAll"
          >
            &#9632; Stop all
          </button>
          <button
            class="sw-btn sw-btn--start"
            :disabled="hasStarting"
            @click="start()"
          >
            &#9654; Start selected
          </button>
        </div>
      </header>
      <div class="services-table">
        <template v-if="coreServices.length > 0">
          <div class="svc-group-label">core</div>
          <ServiceRow
            v-for="def in coreServices"
            :key="def.id"
            :def="def"
            :state="uiState[def.id]"
            @toggle="toggle"
          />
        </template>
        <template v-if="additionalServices.length > 0">
          <div class="svc-group-label">additional</div>
          <ServiceRow
            v-for="def in additionalServices"
            :key="def.id"
            :def="def"
            :state="uiState[def.id]"
            @toggle="toggle"
          />
        </template>
        <template v-if="definitions.length === 0 && !serviceError">
          <div
            v-for="i in SKELETON_ROW_COUNT"
            :key="`skeleton-${i}`"
            class="svc-row-skeleton"
            aria-busy="true"
            aria-label="Loading service"
          >
            <SkeletonBox width="28px" height="15px" radius="8px" />
            <SkeletonBox width="18px" height="18px" radius="4px" />
            <div class="svc-row-skeleton__id">
              <SkeletonBox width="60%" height="12px" />
              <SkeletonBox width="40%" height="9px" />
            </div>
            <SkeletonBox width="70px" height="11px" />
            <SkeletonBox width="50px" height="11px" />
            <SkeletonBox variant="chip" width="40px" height="14px" />
          </div>
        </template>
      </div>
    </section>

    <section class="log-wrap">
      <div class="log-wrap__bar">
        <span class="log-wrap__label">LOGS</span>
        <button class="log-wrap__expand-btn" @click="setView('logs')">expand &#8599;</button>
      </div>
      <LogTail :lines="logLines" max-height="148px" />
    </section>
    </div>
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
        <button class="ctx-btn scope-cancel" @click="scopeRequest = null">Cancel</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.view-dashboard {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
}

.app-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--space-6);
  height: 34px;
  border-bottom: 1px solid var(--border);
  background: var(--bg);
  flex-shrink: 0;
}

.app-strip__brand {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
}

.app-strip__name {
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 700;
  color: var(--accent);
  letter-spacing: 0.02em;
}

.app-strip__tag {
  font-family: var(--font-sans);
  font-size: 10px;
  color: var(--dim);
  letter-spacing: 0.04em;
}

.app-strip__right {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.app-strip__uptime {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
}

.app-strip__settings-btn {
  background: transparent;
  border: none;
  color: var(--dim);
  font-size: 13px;
  cursor: pointer;
  padding: 2px 4px;
  border-radius: var(--radius-xs);
  line-height: 1;
  transition: color 0.1s, background 0.1s;
}

.app-strip__settings-btn:hover {
  color: var(--text);
  background: var(--surface2);
}

.dashboard-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  padding: 20px 28px;
  gap: var(--space-4);
  min-height: 0;
  overflow: hidden;
}

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

.control-block--stats {
  border-color: var(--border);
}

.cb-label {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-family: var(--font-sans);
  font-size: 11px;
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
  font-size: 10px;
  font-weight: 400;
  letter-spacing: 0.04em;
}

.cb-live-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

.ver-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}

.ver-tag {
  font-family: var(--font-mono);
  font-size: 22px;
  font-weight: 400;
  line-height: 1;
}

.ver-patch {
  font-family: var(--font-mono);
  font-size: 14px;
  color: var(--dim);
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
  font-size: 16px;
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
  font-size: 12px;
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
  font-size: 11px;
  color: var(--dim);
}

.vp-suggest-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: color-mix(in srgb, var(--amber) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--amber) 35%, transparent);
  border-radius: var(--radius-xs);
}

.vp-hook-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: color-mix(in srgb, var(--muted) 8%, transparent);
  border: 1px solid var(--border);
  border-radius: var(--radius-xs);
}

.vp-hook-text {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--muted);
  flex: 1;
  min-width: 0;
}

.vp-hook-cmd {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 1px 5px;
  background: var(--surface2);
  border-radius: var(--radius-xs);
  color: var(--text);
}

.vp-hook-copy {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 3px 8px;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius-xs);
  color: var(--muted);
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.1s, color 0.1s;
}

.vp-hook-copy:hover {
  background: var(--dim);
  color: var(--text);
}

.vp-suggest-text {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--amber);
  flex: 1;
  min-width: 0;
}

.vp-suggest-text strong {
  font-family: var(--font-mono);
  font-weight: 600;
}

.vp-suggest-actions {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  flex-shrink: 0;
}

.vp-suggest-btn {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 3px 8px;
  background: color-mix(in srgb, var(--amber) 15%, transparent);
  border: 1px solid color-mix(in srgb, var(--amber) 40%, transparent);
  border-radius: var(--radius-xs);
  color: var(--amber);
  cursor: pointer;
  transition: background 0.1s;
}

.vp-suggest-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--amber) 25%, transparent);
}

.vp-suggest-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.vp-suggest-dismiss {
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--muted);
  font-size: 14px;
  cursor: pointer;
  line-height: 1;
  padding: 0;
  border-radius: var(--radius-xs);
  transition: color 0.1s;
}

.vp-suggest-dismiss:hover {
  color: var(--text);
}

.stat-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-3) var(--space-4);
}

.stat-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.stat-val {
  font-family: var(--font-sans);
  font-size: 18px;
  font-weight: 400;
  line-height: 1;
  font-variant-numeric: tabular-nums;
}

.stat-val--green { color: var(--green); }
.stat-val--amber { color: var(--amber); }
.stat-val--dim   { color: var(--muted); }

.stat-val--small {
  font-size: 14px;
}

.stat-label {
  font-family: var(--font-sans);
  font-size: 10px;
  color: var(--dim);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.docker-down-banner {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 40%, transparent);
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.ddb-icon {
  font-size: 18px;
  color: var(--red);
  line-height: 1;
  flex-shrink: 0;
}

.ddb-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}

.ddb-text strong {
  font-family: var(--font-sans);
  font-size: 12px;
  font-weight: 600;
  color: var(--red);
}

.ddb-sub {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ddb-retry {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 12px;
  background: color-mix(in srgb, var(--red) 18%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 45%, transparent);
  border-radius: var(--radius-xs);
  color: var(--red);
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.1s, border-color 0.1s;
}

.ddb-retry--primary {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  color: var(--accent);
}

.ddb-sub code {
  font-family: var(--font-mono);
  font-size: 10.5px;
  padding: 1px 5px;
  border-radius: var(--radius-xs);
  background: var(--surface2);
  color: var(--text);
}

.ddb-retry:hover:not(:disabled) {
  background: color-mix(in srgb, var(--red) 28%, transparent);
  border-color: color-mix(in srgb, var(--red) 65%, transparent);
}

.ddb-retry:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.port-conflict-banner {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  background: color-mix(in srgb, var(--amber) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--amber) 40%, transparent);
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.pcb-icon {
  font-size: 18px;
  color: var(--amber);
  line-height: 1;
  flex-shrink: 0;
}

.pcb-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}

.pcb-text strong {
  font-family: var(--font-sans);
  font-size: 12px;
  font-weight: 600;
  color: var(--amber);
}

.pcb-sub {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--muted);
}

.pcb-port {
  font-family: var(--font-mono);
  color: var(--text);
}

.pcb-actions {
  display: flex;
  gap: var(--space-2);
  flex-shrink: 0;
}

.pcb-force,
.pcb-dismiss {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 12px;
  border-radius: var(--radius-xs);
  cursor: pointer;
  transition: background 0.1s, border-color 0.1s;
}

.pcb-force {
  background: color-mix(in srgb, var(--amber) 18%, transparent);
  border: 1px solid color-mix(in srgb, var(--amber) 45%, transparent);
  color: var(--amber);
}

.pcb-force:hover {
  background: color-mix(in srgb, var(--amber) 28%, transparent);
  border-color: color-mix(in srgb, var(--amber) 65%, transparent);
}

.pcb-dismiss {
  background: transparent;
  border: 1px solid var(--border);
  color: var(--muted);
}

.pcb-dismiss:hover {
  color: var(--text);
  border-color: var(--dim);
}

.dash-error {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 35%, transparent);
  border-radius: var(--radius-xs);
  color: var(--red);
  font-family: var(--font-mono);
  font-size: 12px;
  flex-shrink: 0;
}

.services-wrap {
  margin: 0;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.sw-header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-5);
  background: transparent;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.sw-title {
  font-family: var(--font-sans);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
}

.sw-counter {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--dim);
  margin-right: auto;
}

.sw-profile {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-right: var(--space-2);
}

.sw-profile__label {
  font-family: var(--font-sans);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--dim);
}

.sw-profile__select {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 3px 8px;
  border-radius: var(--radius-xs);
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--text);
  cursor: pointer;
  max-width: 160px;
  transition: border-color 0.1s;
}

.sw-profile__select:hover {
  border-color: var(--dim);
}

.sw-actions {
  display: flex;
  gap: var(--space-2);
}

.sw-btn {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 3px 10px;
  border-radius: var(--radius-xs);
  cursor: pointer;
  transition: background 0.1s, border-color 0.1s, opacity 0.1s;
  border: 1px solid;
}

.sw-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.sw-btn--start {
  background: color-mix(in srgb, var(--green) 15%, transparent);
  color: var(--green);
  border-color: color-mix(in srgb, var(--green) 45%, transparent);
}

.sw-btn--start:hover:not(:disabled) {
  background: color-mix(in srgb, var(--green) 25%, transparent);
  border-color: color-mix(in srgb, var(--green) 70%, transparent);
}

.sw-btn--stop {
  background: color-mix(in srgb, var(--red) 12%, transparent);
  color: var(--red);
  border-color: color-mix(in srgb, var(--red) 35%, transparent);
}

.sw-btn--stop:hover:not(:disabled) {
  background: color-mix(in srgb, var(--red) 20%, transparent);
  border-color: color-mix(in srgb, var(--red) 60%, transparent);
}

.services-table {
  padding: var(--space-2) 0;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.svc-group-label {
  font-family: var(--font-sans);
  font-size: 10.5px;
  font-weight: 500;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--dim);
  padding: var(--space-2) var(--space-4) var(--space-1);
}

.svc-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 60px;
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--dim);
}

/* Skeleton row — mirror .svc-row grid sehingga real content tidak shift */
.svc-row-skeleton {
  display: grid;
  grid-template-columns: 36px 24px 1fr 100px 68px 80px;
  align-items: center;
  gap: var(--space-3);
  padding: 5px var(--space-4);
  /* Height match-able dengan ServiceRow (svc-row punya font-size 12px + padding 5px) */
}

.svc-row-skeleton__id {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.log-wrap {
  margin: 0;
  height: 184px;
  flex-shrink: 0;
}

.log-wrap__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--space-2) 4px;
}

.log-wrap__label {
  font-family: var(--font-sans);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--dim);
  text-transform: uppercase;
}

.log-wrap__expand-btn {
  font-family: var(--font-sans);
  font-size: 10px;
  background: transparent;
  border: none;
  color: var(--dim);
  cursor: pointer;
  padding: 0;
  transition: color 0.1s;
}

.log-wrap__expand-btn:hover {
  color: var(--accent);
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
  font-size: 12px;
  flex-shrink: 0;
}
.ctx-bar__label { color: var(--muted); }
.ctx-bar__path {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.ctx-bar__spacer { flex: 1; }
.ctx-bar__hint { color: var(--dim); font-size: 11px; white-space: nowrap; }
.ctx-select {
  font-family: var(--font-sans);
  font-size: 12px;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface2);
  color: var(--text);
}
.ctx-btn {
  font-family: var(--font-sans);
  font-size: 12px;
  padding: 4px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  white-space: nowrap;
}
.ctx-btn:hover:not(:disabled) { color: var(--text); }
.ctx-btn--primary {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  color: var(--accent);
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
.ctx-banner__text { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; font-size: 12px; }
.ctx-banner--warn strong { color: var(--amber); }
.ctx-banner--error strong { color: var(--red); }
.ctx-banner__path {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ver-source {
  margin-top: var(--space-2);
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: var(--dim);
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
.scope-dialog__title { font-size: 14px; font-weight: 600; color: var(--text); }
.scope-dialog__body { font-size: 12.5px; color: var(--muted); line-height: 1.55; }
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
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}
.scope-option small {
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: var(--dim);
  word-break: break-all;
}
.scope-option--primary {
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  background: color-mix(in srgb, var(--accent) 10%, var(--surface2));
}
.scope-cancel { align-self: flex-end; }
</style>
