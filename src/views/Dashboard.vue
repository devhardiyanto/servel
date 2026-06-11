<script setup lang="ts">
import { inject, computed } from 'vue'
import { SetViewKey } from '../types/navigation'
import { usePhp } from '@/composables/usePhp'
import { useNode } from '@/composables/useNode'
import { useLogs } from '@/composables/useLogs'
import { useProjectWatch } from '@/composables/useProjectWatch'
import VersionPicker from '@/components/VersionPicker.vue'
import LogTail from '@/components/LogTail.vue'
import StatusBar from '@/components/StatusBar.vue'

const setView = inject(SetViewKey)!

const { watchedPath, watching, pickFolder, error: watchError } = useProjectWatch()

const {
  versions: phpVersions,
  active: phpActive,
  switching: phpSwitching,
  error: phpError,
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
  error: nodeError,
  switchTo: switchNode,
  install: installNode,
  suggested: nodeSuggested,
  suggestedNeedsInstall: nodeSuggestedNeedsInstall,
  dismissSuggestion: dismissNodeSuggestion,
} = useNode()

function shortenPath(path: string): string {
  const parts = path.split(/[/\\]/).filter(Boolean)
  return parts.slice(-2).join('/')
}

const { lines: logLines, clear: clearLogs } = useLogs('PHP')

const phpAccent = '#22d3ee'
const nodeAccent = '#4ade80'

interface PlaceholderService {
  id: string
  name: string
  group: 'core' | 'additional'
  port: string
  ram: number
}

const placeholderServices: PlaceholderService[] = [
  { id: 'mysql',     name: 'MySQL',      group: 'core',       port: ':3306',        ram: 80 },
  { id: 'postgres',  name: 'PostgreSQL', group: 'core',       port: ':5432',        ram: 60 },
  { id: 'redis',     name: 'Redis',      group: 'core',       port: ':6379',        ram: 8 },
  { id: 'rabbitmq',  name: 'RabbitMQ',   group: 'core',       port: ':5672 / 15672', ram: 100 },
  { id: 'mongodb',   name: 'MongoDB',    group: 'additional', port: ':27017',       ram: 120 },
  { id: 'minio',     name: 'MinIO',      group: 'additional', port: ':9000 / 9001', ram: 50 },
  { id: 'mailpit',   name: 'Mailpit',    group: 'additional', port: ':1025 / 8025', ram: 10 },
  { id: 'gotenberg', name: 'Gotenberg',  group: 'additional', port: ':3030',        ram: 60 },
  { id: 'sqlserver', name: 'SQL Server', group: 'additional', port: ':1433',        ram: 600 },
]

const coreServices = computed(() => placeholderServices.filter((s) => s.group === 'core'))
const additionalServices = computed(() => placeholderServices.filter((s) => s.group === 'additional'))
</script>

<template>
  <div class="view view-dashboard">
    <header class="dash-header">
      <div class="dash-brand">
        <span class="dash-brand-name">servel</span>
        <span class="dash-brand-tag">local dev environment</span>
      </div>
      <nav class="dash-nav">
        <button
          class="dash-watch-btn"
          :disabled="watching"
          @click="pickFolder"
        >
          <span v-if="watchedPath">{{ shortenPath(watchedPath) }}</span>
          <span v-else>Watch Project...</span>
        </button>
        <button class="dash-nav-btn" @click="setView('logs')">Logs</button>
        <button class="dash-nav-btn" @click="setView('settings')">Settings</button>
        <button class="dash-nav-btn" @click="setView('onboarding')">Prereq</button>
      </nav>
    </header>

    <section class="control-strip">
      <VersionPicker
        label="PHP Version"
        :accent="phpAccent"
        :versions="phpVersions"
        :active="phpActive"
        :switching="phpSwitching"
        :suggested="phpSuggested"
        :suggested-needs-install="phpSuggestedNeedsInstall"
        @switch="switchPhp"
        @switch-suggested="(v) => switchPhp(v)"
        @install-suggested="(v) => installPhp(v)"
        @dismiss-suggestion="dismissPhpSuggestion"
      />
      <VersionPicker
        label="Node Version"
        :accent="nodeAccent"
        :versions="nodeVersions"
        :active="nodeActive"
        :switching="nodeSwitching"
        :suggested="nodeSuggested"
        :suggested-needs-install="nodeSuggestedNeedsInstall"
        @switch="switchNode"
        @switch-suggested="(v) => switchNode(v)"
        @install-suggested="(v) => installNode(v)"
        @dismiss-suggestion="dismissNodeSuggestion"
      />
    </section>

    <p v-if="phpError || nodeError || watchError" class="dash-error">
      {{ phpError || nodeError || watchError }}
    </p>

    <section class="services-wrap">
      <header class="sw-header">
        <span class="sw-title">Services</span>
        <span class="sw-stub">read-only · diisi Phase 3</span>
      </header>
      <div class="services-table">
        <template v-for="group in (['core', 'additional'] as const)" :key="group">
          <div class="svc-group-label">{{ group }}</div>
          <div
            v-for="svc in (group === 'core' ? coreServices : additionalServices)"
            :key="svc.id"
            class="svc-row svc-row--inert"
          >
            <span class="svc-name">{{ svc.name }}</span>
            <span class="svc-port">{{ svc.port }}</span>
            <span class="svc-ram">~{{ svc.ram }} MB</span>
            <span class="svc-status">off</span>
          </div>
        </template>
      </div>
    </section>

    <section class="log-wrap">
      <LogTail :lines="logLines" max-height="180px" @clear="clearLogs" />
    </section>

    <StatusBar :running-count="0" :total-ram-mb="0" uptime="—" />
  </div>
</template>

<style scoped>
.view-dashboard {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
}

.dash-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-4) var(--space-6);
  border-bottom: 1px solid var(--border);
}

.dash-brand {
  display: flex;
  align-items: baseline;
  gap: var(--space-3);
}

.dash-brand-name {
  font-family: var(--font-mono);
  font-size: 18px;
  font-weight: 700;
  color: var(--accent);
  letter-spacing: 0.02em;
}

.dash-brand-tag {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--dim);
  letter-spacing: 0.04em;
}

.dash-nav {
  display: flex;
  gap: var(--space-2);
}

.dash-nav-btn {
  font-family: var(--font-sans);
  font-size: 12px;
  padding: 5px 12px;
  background: var(--surface);
  color: var(--muted);
  border: 1px solid var(--border);
  border-radius: 4px;
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s;
}

.dash-nav-btn:hover {
  color: var(--text);
  border-color: var(--muted);
}

.dash-watch-btn {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 5px 10px;
  background: var(--surface);
  color: var(--muted);
  border: 1px solid var(--border);
  border-radius: 4px;
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s;
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dash-watch-btn:hover:not(:disabled) {
  color: var(--text);
  border-color: var(--accent);
}

.dash-watch-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.control-strip {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-4);
  padding: var(--space-6);
}

.dash-error {
  margin: 0 var(--space-6) var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 35%, transparent);
  border-radius: 4px;
  color: var(--red);
  font-family: var(--font-mono);
  font-size: 12px;
}

.services-wrap {
  margin: 0 var(--space-6) var(--space-4);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}

.sw-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  background: var(--surface2);
  border-bottom: 1px solid var(--border);
}

.sw-title {
  font-family: var(--font-sans);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
}

.sw-stub {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
}

.services-table {
  padding: var(--space-2) 0;
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

.svc-row {
  display: grid;
  grid-template-columns: 1fr 1fr 80px 50px;
  align-items: center;
  gap: var(--space-3);
  padding: 6px var(--space-4);
  font-family: var(--font-mono);
  font-size: 12px;
}

.svc-row--inert {
  color: var(--dim);
  opacity: 0.7;
}

.svc-name {
  color: var(--muted);
}

.svc-port {
  color: var(--dim);
}

.svc-ram {
  color: var(--amber);
  text-align: right;
}

.svc-status {
  color: var(--dim);
  text-align: right;
  letter-spacing: 0.04em;
}

.log-wrap {
  margin: 0 var(--space-6) var(--space-4);
}
</style>
