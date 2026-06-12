<script setup lang="ts">
import { inject, computed } from 'vue'
import { SetViewKey } from '../types/navigation'
import { usePrereq } from '@/composables/usePrereq'
import PrereqCard from '@/components/PrereqCard.vue'
import type { PrereqAction } from '@/components/PrereqCard.vue'

const setView = inject(SetViewKey)!

const {
  status,
  checking,
  startingDocker,
  startDockerError,
  allReady,
  dockerInstalledButNotRunning,
  check,
  startDocker,
} = usePrereq()

const readyCount = computed<number>(() => {
  if (!status.value) return 0
  const s = status.value
  return [
    s.docker_installed && s.docker_running,
    s.phpvm_installed,
    s.fnm_installed,
  ].filter(Boolean).length
})

const totalCount = 3

const progressPct = computed<number>(() => (readyCount.value / totalCount) * 100)

const isLinuxDockerError = computed<boolean>(() =>
  startDockerError.value?.includes('systemctl') ?? false
)

function openUrl(url: string): void {
  window.open(url, '_blank', 'noopener')
}

const dockerActions = computed<PrereqAction[]>(() => [
  {
    label: '⬇ Download Docker Desktop',
    primary: true,
    onClick: () => openUrl('https://www.docker.com/products/docker-desktop/'),
  },
  {
    label: 'WSL2 + Docker Engine Guide ↗',
    onClick: () => openUrl('https://docs.docker.com/desktop/wsl/'),
  },
])

const phpvmActions = computed<PrereqAction[]>(() => [
  {
    label: 'View phpvm on GitHub ↗',
    onClick: () => openUrl('https://github.com/devhardiyanto/phpvm'),
  },
])

const fnmActions = computed<PrereqAction[]>(() => [
  {
    label: 'View fnm on GitHub ↗',
    onClick: () => openUrl('https://github.com/Schniz/fnm'),
  },
])
</script>

<template>
  <div class="ob-viewport">
    <div class="ob-main">
      <div class="ob-inner">
        <div class="ob-mark">
          <span class="ob-mark-sq ob-mark-sq--filled"></span>
          <span class="ob-mark-sq ob-mark-sq--outline"></span>
        </div>

        <h1 class="ob-heading">Before we start</h1>
        <p class="ob-sub">Servel needs a few things installed to work.</p>

        <div class="ob-progress">
          <div class="ob-progress-track">
            <div
              class="ob-progress-fill"
              :class="{ 'ob-progress-fill--done': allReady }"
              :style="{ width: progressPct + '%' }"
            ></div>
          </div>
          <span class="ob-progress-label">{{ readyCount }}/{{ totalCount }} ready</span>
        </div>

        <div class="ob-cards">
          <PrereqCard
            name="Docker"
            :installed="status?.docker_installed ?? false"
            :running="status?.docker_running"
            icon="&#x1F433;"
            ok-desc="Docker Engine detected and running"
            not-found-desc="Required to run infra services (MySQL, Redis, etc)"
            :actions="dockerActions"
          >
            <template v-if="status?.docker_installed && !status?.docker_running">
              Docker installed but not running
            </template>
          </PrereqCard>

          <!-- TODO Phase 4: replace hardcoded version with real value from prereq_check command -->
          <PrereqCard
            name="phpvm"
            :installed="status?.phpvm_installed ?? false"
            icon="&#x1F418;"
            ok-desc="PHP version manager &#x2014; v1.7.0 detected"
            not-found-desc="PHP version manager"
            :actions="phpvmActions"
          />

          <!-- TODO Phase 4: replace hardcoded version with real value from prereq_check command -->
          <PrereqCard
            name="fnm"
            :installed="status?.fnm_installed ?? false"
            icon="&#x26A1;"
            ok-desc="Fast Node version manager &#x2014; v1.37.1 detected"
            not-found-desc="Fast Node version manager, built in Rust"
            :actions="fnmActions"
            code="winget install Schniz.fnm"
          />
        </div>

        <div v-if="startDockerError && !isLinuxDockerError" class="ob-error">
          {{ startDockerError }}
        </div>

        <div v-if="isLinuxDockerError" class="ob-instruction">
          <p class="ob-instruction__title">Start Docker manually:</p>
          <code class="ob-instruction__cmd">sudo systemctl start docker</code>
          <p class="ob-instruction__hint">Then click "Refresh checks" to refresh the status.</p>
        </div>

        <p class="ob-note">Already installed everything? Click Refresh to re-check.</p>

        <div class="ob-cta-row">
          <button
            v-if="dockerInstalledButNotRunning"
            class="ob-btn ob-btn--primary"
            :disabled="startingDocker"
            @click="startDocker"
          >
            <span v-if="startingDocker" class="ob-spinner"></span>
            {{ startingDocker ? 'Starting Docker...' : 'Start Docker' }}
          </button>

          <button
            v-if="allReady"
            class="ob-btn ob-btn--ready"
            @click="setView('dashboard')"
          >
            Continue to Servel &#x2192;
          </button>

          <button
            v-if="!allReady"
            class="ob-btn ob-btn--disabled"
            disabled
          >
            Continue to Servel &#x2192;
          </button>

          <button
            class="ob-btn ob-btn--ghost"
            :disabled="checking"
            @click="check"
          >
            <span v-if="checking" class="ob-spinner"></span>
            {{ checking ? 'Checking...' : '&#x21BB; Refresh checks' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ob-viewport {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font-sans);
}

.ob-main {
  flex: 1;
  overflow-y: auto;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 48px var(--space-4) var(--space-8);
}

.ob-inner {
  width: 100%;
  max-width: 560px;
}

.ob-mark {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-bottom: var(--space-6);
}

.ob-mark-sq {
  width: 14px;
  height: 14px;
  border-radius: 3px;
}

.ob-mark-sq--filled {
  background: var(--accent);
}

.ob-mark-sq--outline {
  border: 2px solid color-mix(in srgb, var(--accent) 50%, transparent);
}

.ob-heading {
  font-size: 22px;
  font-weight: 600;
  color: var(--text);
  margin: 0 0 var(--space-2) 0;
}

.ob-sub {
  font-size: 14px;
  color: var(--muted);
  margin: 0 0 var(--space-6) 0;
}

.ob-progress {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-6);
}

.ob-progress-track {
  flex: 1;
  height: 4px;
  background: var(--surface2);
  border-radius: 2px;
  overflow: hidden;
}

.ob-progress-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 2px;
  transition: width 0.3s ease, background 0.3s ease;
}

.ob-progress-fill--done {
  background: var(--green);
}

.ob-progress-label {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
  flex-shrink: 0;
}

.ob-cards {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  margin-bottom: var(--space-6);
}

.ob-error {
  padding: var(--space-3) var(--space-4);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 30%, transparent);
  border-radius: 8px;
  font-size: 13px;
  color: var(--red);
  margin-bottom: var(--space-4);
}

.ob-instruction {
  padding: var(--space-4);
  background: color-mix(in srgb, var(--amber) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--amber) 25%, transparent);
  border-radius: 8px;
  margin-bottom: var(--space-4);
}

.ob-instruction__title {
  font-size: 13px;
  font-weight: 600;
  color: var(--amber);
  margin: 0 0 var(--space-2) 0;
}

.ob-instruction__cmd {
  display: block;
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--text);
  background: var(--surface2);
  padding: var(--space-2) var(--space-3);
  border-radius: 6px;
  margin-bottom: var(--space-2);
}

.ob-instruction__hint {
  font-size: 12px;
  color: var(--muted);
  margin: 0;
}

.ob-note {
  font-size: 13px;
  color: var(--muted);
  margin: 0 0 var(--space-4) 0;
}

.ob-cta-row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: center;
}

.ob-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-6);
  border-radius: 6px;
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  border: 1px solid transparent;
  transition: opacity 0.15s, background 0.15s;
}

.ob-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.ob-btn--primary {
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  color: var(--accent);
}

.ob-btn--ready {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--bg);
}

.ob-btn--disabled {
  background: var(--surface);
  border-color: var(--border);
  color: var(--dim);
  cursor: not-allowed;
}

.ob-btn--ghost {
  background: var(--surface);
  border-color: var(--border);
  color: var(--muted);
}

.ob-btn--ghost:not(:disabled):hover {
  color: var(--text);
  border-color: var(--surface2);
}

.ob-spinner {
  display: inline-block;
  width: 12px;
  height: 12px;
  border: 2px solid currentColor;
  border-top-color: transparent;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
