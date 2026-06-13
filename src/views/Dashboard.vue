<script setup lang="ts">
import { onMounted, onUnmounted, computed, inject, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { usePhp } from '@/composables/usePhp'
import { usePhpHookStatus } from '@/composables/usePhpHookStatus'
import { useNode } from '@/composables/useNode'
import { useLogs } from '@/composables/useLogs'
import { useServices } from '@/composables/useServices'
import { useConfig } from '@/composables/useConfig'
import { useUptime } from '@/composables/useUptime'
import { SetViewKey } from '@/types/navigation'
import LogTail from '@/components/LogTail.vue'
import ServiceRow from '@/components/ServiceRow.vue'

const {
  versions: phpVersions,
  active: phpActive,
  switching: phpSwitching,
  switchTo: switchPhp,
  install: installPhp,
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
  suggested: nodeSuggested,
  suggestedNeedsInstall: nodeSuggestedNeedsInstall,
  dismissSuggestion: dismissNodeSuggestion,
} = useNode()

const setView = inject(SetViewKey)!
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

const phpAccent = '#22d3ee'
const nodeAccent = '#4ade80'
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
  load: loadServices,
  toggle,
  setSelectedIds,
  start,
  stopAll,
} = useServices()

const totalRamWithBaseline = computed(() => {
  const runningRam = runningIds.value.reduce((acc, id) => {
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

function nowTs(): string {
  return new Date().toTimeString().slice(0, 8)
}

onMounted(async () => {
  await loadServices()
  void refreshPhpHookStatus()
  await useConfig().load()

  const cfg = useConfig().config.value
  const { push: pushLog } = useLogs('ENV')

  // Restore selection UI dari config
  if (cfg.selectedServiceIds.length > 0) {
    setSelectedIds(cfg.selectedServiceIds)
  }

  // Diagnostic + auto-start path
  if (!cfg.autoStart) {
    pushLog({ ts: nowTs(), src: 'ENV', text: 'auto-start: disabled' })
  } else if (cfg.selectedServiceIds.length === 0) {
    pushLog({ ts: nowTs(), src: 'ENV', text: 'auto-start: no saved selection' })
  } else {
    let dockerRunning = false
    try {
      const prereqResult = await invoke<import('@/types/prereq').PrereqStatus>('check_prerequisites')
      dockerRunning = prereqResult.docker_running
    } catch {
      dockerRunning = false
    }

    if (!dockerRunning) {
      pushLog({ ts: nowTs(), src: 'ENV', text: 'auto-start: Docker not ready — skipped' })
    } else {
      await start(cfg.selectedServiceIds)
      pushLog({ ts: nowTs(), src: 'ENV', text: `auto-started: ${cfg.selectedServiceIds.join(', ')}` })
    }
  }

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
        <div v-if="definitions.length === 0" class="svc-loading">loading&#x2026;</div>
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
  height: 28px;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  background: var(--surface);
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
  border-radius: 3px;
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
  border-radius: 8px;
  padding: var(--space-4);
  transition: border-color 0.15s;
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
  border-radius: 4px;
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
  border-radius: 4px;
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
  font-family: var(--font-mono);
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
  border-radius: 4px;
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
  border-radius: 4px;
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
  border-radius: 3px;
  color: var(--text);
}

.vp-hook-copy {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 3px 8px;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: 3px;
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
  border-radius: 3px;
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
  border-radius: 3px;
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
  font-family: var(--font-mono);
  font-size: 18px;
  font-weight: 400;
  line-height: 1;
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

.dash-error {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 35%, transparent);
  border-radius: 4px;
  color: var(--red);
  font-family: var(--font-mono);
  font-size: 12px;
  flex-shrink: 0;
}

.services-wrap {
  margin: 0;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
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
  padding: var(--space-3) var(--space-4);
  background: #0d0d11;
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
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
  margin-right: auto;
}

.sw-actions {
  display: flex;
  gap: var(--space-2);
}

.sw-btn {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 3px 10px;
  border-radius: 4px;
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
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.08em;
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
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--dim);
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
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--dim);
  text-transform: uppercase;
}

.log-wrap__expand-btn {
  font-family: var(--font-mono);
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
</style>
