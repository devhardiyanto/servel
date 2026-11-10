<script setup lang="ts">
import { inject, computed, ref, onMounted } from 'vue'
import { SetViewKey } from '../types/navigation'
import { usePrereq } from '@/composables/usePrereq'
import { useTauri } from '@/composables/useTauri'
import { useConfig } from '@/composables/useConfig'
import PrereqCard from '@/components/PrereqCard.vue'
import type { PrereqAction } from '@/components/PrereqCard.vue'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'

const setView = inject(SetViewKey)!

// Platform dipakai untuk menampilkan instruksi prasyarat yang sesuai OS.
// Default "windows" (target utama) sampai command resolve.
const { call } = useTauri()
const platform = ref<string>('windows')
onMounted(async () => {
  const p = await call<string>('get_platform')
  if (p) platform.value = p
})
const isLinux = computed<boolean>(() => platform.value === 'linux')

const {
  status,
  checking,
  startingEngine,
  startEngineError,
  allReady,
  canContinue,
  engineReady,
  engineInstalledButNotRunning,
  engineLabel,
  canSwitchToWslc,
  check,
  startEngine,
} = usePrereq()

const isWindows = computed<boolean>(() => platform.value === 'windows')
const isWslc = computed<boolean>(() => status.value?.engine === 'wslc')

const { setContainerRuntime } = useConfig()
const switching = ref(false)
async function useEngine(kind: 'wslc' | 'docker'): Promise<void> {
  switching.value = true
  try {
    await setContainerRuntime(kind)
    await check()
  } finally {
    switching.value = false
  }
}

// Dengan mise terpasang (v1.7), satu kartu mise menggantikan phpvm + fnm.
const usesMise = computed<boolean>(() => !!status.value?.mise_installed)

const readyCount = computed<number>(() => {
  if (!status.value) return 0
  const s = status.value
  const items = usesMise.value
    ? [engineReady.value, s.mise_installed]
    : [engineReady.value, s.phpvm_installed, s.fnm_installed]
  return items.filter(Boolean).length
})

const totalCount = computed<number>(() => (usesMise.value ? 2 : 3))

const miseOkDesc = computed<string>(() => {
  const v = status.value?.mise_version
  return `Manages PHP and Node per project${v ? ` — v${v} detected` : ''}`
})

const progressPct = computed<number>(() => (readyCount.value / totalCount.value) * 100)

function openUrl(url: string): void {
  window.open(url, '_blank', 'noopener')
}

const WSL_RELEASES_URL = 'https://github.com/microsoft/WSL/releases'

const engineActions = computed<PrereqAction[]>(() => {
  if (isWindows.value) {
    if (isWslc.value) {
      return [
        { label: 'WSL release notes ↗', primary: true, onClick: () => openUrl(WSL_RELEASES_URL) },
        { label: 'Use Docker Desktop instead', onClick: () => void useEngine('docker') },
      ]
    }
    if (status.value?.wslc_available) {
      return [{ label: 'Use WSL containers', primary: true, onClick: () => void useEngine('wslc') }]
    }
    return [
      { label: 'Use WSL containers', primary: true, onClick: () => void useEngine('wslc') },
      {
        label: 'Docker Desktop ↗',
        onClick: () => openUrl('https://www.docker.com/products/docker-desktop/'),
      },
    ]
  }
  if (isLinux.value) {
    return [
      {
        label: '⬇ Install Docker Engine',
        primary: true,
        onClick: () => openUrl('https://docs.docker.com/engine/install/'),
      },
      {
        label: 'Post-install (grup docker / rootless) ↗',
        onClick: () => openUrl('https://docs.docker.com/engine/install/linux-postinstall/'),
      },
    ]
  }
  if (platform.value === 'macos') {
    return [
      {
        label: '⬇ Download Docker Desktop',
        primary: true,
        onClick: () => openUrl('https://www.docker.com/products/docker-desktop/'),
      },
    ]
  }
  return [
    {
      label: '⬇ Download Docker Desktop',
      primary: true,
      onClick: () => openUrl('https://www.docker.com/products/docker-desktop/'),
    },
    {
      label: 'WSL2 + Docker Engine Guide ↗',
      onClick: () => openUrl('https://docs.docker.com/desktop/wsl/'),
    },
  ]
})

const engineIcon = computed<string>(() => (isWslc.value ? '\u{1F4E6}' : '\u{1F433}'))

const engineOkDesc = computed<string>(() => {
  const v = status.value?.engine_version
  if (isWslc.value) {
    return `Built into WSL${v ? ` — wslc v${v}` : ''}. No Docker Desktop needed.`
  }
  return v ? `Docker v${v} detected and running` : 'Docker Engine detected and running'
})

const engineMissingDesc = computed<string>(() => {
  if (isWindows.value) {
    return isWslc.value
      ? 'WSL containers need WSL 3.0.1 or newer. Update WSL, then refresh.'
      : 'Needed to run services. Built-in WSL containers (WSL 3.0.1+) replace Docker Desktop.'
  }
  return 'Required to run infra services (MySQL, Redis, etc)'
})

// `wsl --update` cukup untuk mendapatkan wslc di Windows.
const engineCode = computed<string | undefined>(() =>
  isWindows.value && !status.value?.engine_installed ? 'wsl --update' : undefined,
)

const showElevatedWarning = computed<boolean>(
  () => isWindows.value && isWslc.value && !!status.value?.elevated,
)

// Perintah install fnm per-OS untuk ditampilkan di code-block prereq card.
const fnmInstallCmd = computed<string>(() => {
  switch (platform.value) {
    case 'macos':
      return 'brew install fnm'
    case 'linux':
      return 'curl -fsSL https://fnm.vercel.app/install | bash'
    default:
      return 'winget install Schniz.fnm'
  }
})

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

// Tanpa phpvm/fnm user tetap bisa lanjut: mise (atau tool lain) bisa mengelola
// versi PHP/Node. Pesan "opsional" menjelaskan kenapa kartu merah tidak memblokir.
const optionalHint = computed<string>(() => {
  const m = status.value?.mise_version
  return status.value?.mise_installed
    ? ` Optional — mise${m ? ` v${m}` : ''} detected.`
    : ' Optional — or install mise to manage PHP and Node per project.'
})

const phpvmOkDesc = computed<string>(() => {
  const v = status.value?.phpvm_version
  return v
    ? `PHP version manager — v${v} detected`
    : 'PHP version manager'
})

const fnmOkDesc = computed<string>(() => {
  const v = status.value?.fnm_version
  return v
    ? `Fast Node version manager — v${v} detected`
    : 'Fast Node version manager'
})
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

        <div v-if="status === null" class="ob-cards" aria-busy="true" aria-label="Checking prerequisites">
          <div v-for="i in 3" :key="`prereq-skel-${i}`" class="prereq-skel">
            <SkeletonBox width="36px" height="36px" radius="8px" />
            <div class="prereq-skel__body">
              <div class="prereq-skel__row">
                <SkeletonBox width="100px" height="14px" />
                <SkeletonBox variant="chip" width="76px" height="16px" />
              </div>
              <SkeletonBox width="70%" height="11px" />
            </div>
          </div>
        </div>

        <div v-else class="ob-cards">
          <PrereqCard
            :name="engineLabel"
            :installed="status?.engine_installed ?? false"
            :running="status?.engine_running"
            :icon="engineIcon"
            :ok-desc="engineOkDesc"
            :not-found-desc="engineMissingDesc"
            :actions="engineActions"
            :code="engineCode"
          >
            <template v-if="engineInstalledButNotRunning">
              {{ isWslc ? ' WSL containers are installed but not responding.' : ' Docker is installed but not running.' }}
            </template>
          </PrereqCard>

          <PrereqCard
            v-if="usesMise"
            name="mise"
            :installed="true"
            icon="&#x1F9F0;"
            :ok-desc="miseOkDesc"
          />

          <PrereqCard
            v-if="!usesMise"
            name="phpvm"
            :installed="status?.phpvm_installed ?? false"
            icon="&#x1F418;"
            :ok-desc="phpvmOkDesc"
            :not-found-desc="'PHP version manager.' + optionalHint"
            :actions="phpvmActions"
          />

          <PrereqCard
            v-if="!usesMise"
            name="fnm"
            :installed="status?.fnm_installed ?? false"
            icon="&#x26A1;"
            :ok-desc="fnmOkDesc"
            :not-found-desc="'Fast Node version manager.' + optionalHint"
            :actions="fnmActions"
            :code="fnmInstallCmd"
          />
        </div>

        <div v-if="showElevatedWarning" class="ob-error ob-error--soft">
          Servel is running as Administrator. WSL containers keep a separate session per
          elevation, so containers created from a normal window will not appear here.
          Restart Servel without "Run as administrator".
        </div>

        <div v-if="startEngineError && !isLinux" class="ob-error">
          {{ startEngineError }}
        </div>

        <div v-if="isLinux && engineInstalledButNotRunning" class="ob-instruction">
          <p class="ob-instruction__title">Start Docker manually:</p>
          <code class="ob-instruction__cmd">sudo systemctl start docker</code>
          <p class="ob-instruction__hint">Lalu klik "Refresh checks" untuk perbarui status.</p>
        </div>

        <p class="ob-note">Already installed everything? Click Refresh to re-check.</p>

        <div class="ob-cta-row">
          <button
            v-if="engineInstalledButNotRunning && !isLinux"
            class="ob-btn ob-btn--primary"
            :disabled="startingEngine"
            @click="startEngine"
          >
            <span v-if="startingEngine" class="ob-spinner"></span>
            <template v-if="isWslc">{{ startingEngine ? 'Connecting...' : 'Retry WSL containers' }}</template>
            <template v-else>{{ startingEngine ? 'Starting Docker...' : 'Start Docker' }}</template>
          </button>

          <button
            v-if="canSwitchToWslc && engineInstalledButNotRunning"
            class="ob-btn ob-btn--ghost"
            :disabled="switching"
            @click="useEngine('wslc')"
          >
            Use WSL containers instead
          </button>

          <button
            v-if="canContinue"
            class="ob-btn ob-btn--ready"
            @click="setView('dashboard')"
          >
            Continue to Servel &#x2192;
          </button>

          <button
            v-if="!canContinue"
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
  /* Tinggi tetap, bukan min-height: body overflow hidden, jadi scroll harus
     terjadi di .ob-main. Dengan min-height viewport ikut tumbuh dan konten
     terpotong saat jendela tidak di-maximize. */
  height: 100vh;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font-sans);
}

.ob-main {
  flex: 1;
  min-height: 0;
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
  border-radius: var(--radius-xs);
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
  font-family: var(--font-sans);
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

/* Skeleton mirror layout PrereqCard supaya tidak ada layout shift saat status arrive */
.prereq-skel {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
}

.prereq-skel__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-top: 4px;
}

.prereq-skel__row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.ob-error {
  padding: var(--space-3) var(--space-4);
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--red) 30%, transparent);
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--red);
  margin-bottom: var(--space-4);
}

.ob-error--soft {
  background: color-mix(in srgb, var(--amber) 10%, transparent);
  border-color: color-mix(in srgb, var(--amber) 30%, transparent);
  color: var(--amber);
  line-height: 1.55;
}

.ob-instruction {
  padding: var(--space-4);
  background: color-mix(in srgb, var(--amber) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--amber) 25%, transparent);
  border-radius: var(--radius-md);
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
  border-radius: var(--radius-sm);
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
  border-radius: var(--radius-sm);
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
