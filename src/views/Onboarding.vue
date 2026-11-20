<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import { inject, computed, ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { SetViewKey } from '../types/navigation'
import { usePrereq } from '@/composables/usePrereq'
import { useTauri } from '@/composables/useTauri'
import { useConfig } from '@/composables/useConfig'
import PrereqCard from '@/components/PrereqCard.vue'
import type { PrereqAction } from '@/components/PrereqCard.vue'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import KovaMark from '@/components/shell/KovaMark.vue'
import AddProjectForm from '@/components/AddProjectForm.vue'
import { open as openFolderDialog } from '@tauri-apps/plugin-dialog'

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
  canContinue,
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

// v3: mise satu-satunya pengelola versi PHP/Node.
const usesMise = computed<boolean>(() => !!status.value?.mise_installed)

const miseOkDesc = computed<string>(() => {
  const v = status.value?.mise_version
  return `Manages PHP and Node per project${v ? ` — v${v} detected` : ''}`
})

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
      return [
        { label: 'Use WSL containers', primary: true, onClick: () => void useEngine('wslc') },
        {
          label: 'Docker Desktop ↗',
          onClick: () => openUrl('https://www.docker.com/products/docker-desktop/'),
        },
      ]
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
    if (isWslc.value) {
      const v = status.value?.wsl_version
      return `WSL containers need WSL 3.0.1 or newer${v ? ` (this PC has WSL ${v})` : ''}. Update WSL, then refresh.`
    }
    // v3.7.2: WSL containers sudah terpasang, jadi tidak perlu `wsl --update`.
    if (status.value?.wslc_available) {
      const v = status.value?.wsl_version
      return `Docker is not installed, but WSL containers${v ? ` (WSL ${v})` : ''} are already on this PC and can run your services without Docker Desktop.`
    }
    return 'Needed to run services. Built-in WSL containers (WSL 3.0.1+) replace Docker Desktop.'
  }
  return 'Required to run infra services (MySQL, Redis, etc)'
})

// `wsl --update` cukup untuk mendapatkan wslc di Windows.
// Tidak ditampilkan bila WSL containers sudah tersedia (memilih Docker lalu
// menjalankan setup lagi dulu tetap menyarankan `wsl --update`).
const engineCode = computed<string | undefined>(() =>
  isWindows.value && !status.value?.engine_installed && !status.value?.wslc_available ? 'wsl --update' : undefined,
)

const showElevatedWarning = computed<boolean>(
  () => isWindows.value && isWslc.value && !!status.value?.elevated,
)

// mise belum ada (v2.11): Windows dipasang satu klik lewat winget; macOS/Linux
// diberi perintah resminya.
const installingMise = ref(false)
const miseInstallMsg = ref('')
const miseInstallErr = ref('')
async function installMise(): Promise<void> {
  installingMise.value = true
  miseInstallErr.value = ''
  miseInstallMsg.value = ''
  try {
    miseInstallMsg.value = await invoke<string>('mise_install_winget')
    await check()
  } catch (err) {
    miseInstallErr.value = String(err)
  } finally {
    installingMise.value = false
  }
}
const miseActions = computed<PrereqAction[]>(() => [
  ...(isWindows.value && !miseManual.value
    ? [{ label: installingMise.value ? 'Installing…' : 'Install mise', primary: true, onClick: () => void installMise() }]
    : []),
  ...(isWindows.value && !miseManual.value
    ? [{ label: "I'll install it myself", onClick: () => (miseManual.value = true) }]
    : []),
  { label: 'mise.jdx.dev ↗', onClick: () => openUrl('https://mise.jdx.dev/getting-started.html') },
])
// v3.4 (canvas Flow A): tiga langkah — Engine, Runtimes, First project.
const step = ref<1 | 2 | 3>(1)
const steps = [
  { n: 1, label: 'Engine' },
  { n: 2, label: 'Runtimes' },
  { n: 3, label: 'First project' },
] as const
/** "I'll install it myself": tampilkan perintahnya, lalu Check again / Skip. */
const miseManual = ref(false)

const { config: cfg, markOnboarded } = useConfig()
const hasProjects = computed(() => cfg.value.projects.length > 0)
const firstPath = ref('')
const firstForm = ref<InstanceType<typeof AddProjectForm> | null>(null)
const pickErr = ref('')

function finish(): void {
  void markOnboarded()
  setView('projects')
}

function afterRuntimes(): void {
  if (hasProjects.value) finish()
  else step.value = 3
}

async function chooseFirstFolder(): Promise<void> {
  pickErr.value = ''
  try {
    const picked = await openFolderDialog({ directory: true, multiple: false, title: 'Choose your first project folder' })
    if (typeof picked === 'string') firstPath.value = picked
  } catch (err) {
    pickErr.value = String(err)
  }
}

async function addFirst(start: boolean): Promise<void> {
  const p = await firstForm.value?.save(start)
  if (p) finish()
}

const miseInstallCmd = computed<string>(() => {
  if (isWindows.value) return 'winget install jdx.mise'
  if (platform.value === 'macos') return 'brew install mise'
  return 'curl https://mise.run | sh'
})
</script>

<template>
  <div class="ob-viewport">
    <div class="ob-main">
      <div class="ob-inner">
        <div class="ob-mark">
          <KovaMark :size="40" />
        </div>

        <h1 class="ob-heading">Welcome to Kova</h1>
        <p class="ob-sub">
          Every local project, running in one click. Kova checks two things first: a container engine for
          your services, and mise for PHP, Node, Go and Python versions.
        </p>

        <ol class="ob-steps" aria-label="Setup steps">
          <li
            v-for="st in steps"
            :key="st.n"
            class="ob-step"
            :class="{ 'ob-step--on': step === st.n, 'ob-step--done': step > st.n }"
            :aria-current="step === st.n ? 'step' : undefined"
          >
            <span class="ob-step__n">{{ step > st.n ? '✓' : st.n }}</span>{{ st.label }}
          </li>
        </ol>

        <div v-if="status === null && step < 3" class="ob-cards" aria-busy="true" aria-label="Checking prerequisites">
          <div v-for="i in 2" :key="`prereq-skel-${i}`" class="prereq-skel">
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

        <div v-else-if="step === 1" class="ob-cards">
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
        </div>

        <div v-else-if="step === 2" class="ob-cards">
          <PrereqCard
            v-if="usesMise"
            name="mise"
            :installed="true"
            icon="&#x1F9F0;"
            :ok-desc="miseOkDesc"
          />

          <PrereqCard
            v-if="!usesMise"
            name="mise"
            :installed="false"
            icon="&#x1F9F0;"
            not-found-desc="Manages PHP, Node, Go and Python per project. Needed to pick versions; you can continue without it."
            :actions="miseActions"
            :code="!isWindows || miseManual ? miseInstallCmd : undefined"
          />
          <p v-if="miseInstallMsg" class="ob-note">{{ miseInstallMsg }}</p>
          <p v-if="miseInstallErr" class="ob-error ob-error--soft">{{ miseInstallErr }}</p>
        </div>

        <div v-else-if="step === 3" class="ob-cards">
          <div class="ob-first">
            <h2 class="ob-first__title">Add your first project</h2>
            <template v-if="!firstPath">
              <p class="ob-first__text">Pick a folder. Kova detects how to run it and gives it a fixed localhost port.</p>
              <p class="ob-first__kinds">Laravel, WordPress, PHP, Django, FastAPI, Flask, Vite, Next, Nuxt, Angular, Node, Go</p>
              <p v-if="pickErr" class="ob-error ob-error--soft">{{ pickErr }}</p>
            </template>
            <AddProjectForm v-else ref="firstForm" :key="firstPath" :path="firstPath" compact />
          </div>
        </div>

        <div v-if="step === 1 && showElevatedWarning" class="ob-error ob-error--soft">
          Kova is running as Administrator. WSL containers keep a separate session per
          elevation, so containers created from a normal window will not appear here.
          Restart Kova without "Run as administrator".
        </div>

        <div v-if="step === 1 && startEngineError && !isLinux" class="ob-error">
          {{ startEngineError }}
        </div>

        <div v-if="step === 1 && isLinux && engineInstalledButNotRunning" class="ob-instruction">
          <p class="ob-instruction__title">Start Docker manually:</p>
          <code class="ob-instruction__cmd">sudo systemctl start docker</code>
          <p class="ob-instruction__hint">Then click "Refresh checks" to update the status.</p>
        </div>

        <p v-if="step === 1" class="ob-note">Already installed everything? Click Refresh to re-check.</p>

        <!-- Langkah 2: mise -->
        <div v-if="step === 2" class="ob-cta-row">
          <UiButton variant="quiet" size="lg" @click="step = 1">Back</UiButton>
          <UiButton v-if="usesMise" variant="primary" size="lg" @click="afterRuntimes">Continue &#x2192;</UiButton>
          <template v-else>
            <UiButton size="lg" :disabled="checking || installingMise" @click="check">
              {{ checking ? 'Checking…' : 'Check again' }}
            </UiButton>
            <UiButton variant="quiet" size="lg" :disabled="installingMise" @click="afterRuntimes">Skip for now</UiButton>
          </template>
        </div>

        <!-- Langkah 3: project pertama -->
        <div v-else-if="step === 3" class="ob-cta-row">
          <UiButton variant="quiet" size="lg" @click="firstPath ? (firstPath = '') : (step = 2)">Back</UiButton>
          <template v-if="!firstPath">
            <UiButton variant="quiet" size="lg" @click="finish">Skip for now</UiButton>
            <UiButton variant="primary" size="lg" @click="chooseFirstFolder">Choose folder</UiButton>
          </template>
          <template v-else>
            <UiButton size="lg" :disabled="!firstForm?.ready || firstForm?.saving" @click="addFirst(false)">Add</UiButton>
            <UiButton
              variant="primary"
              size="lg"
              :disabled="!firstForm?.ready || !firstForm?.canStart || firstForm?.saving"
              @click="addFirst(true)"
            >Add &amp; start</UiButton>
          </template>
        </div>

        <div v-else class="ob-cta-row">
          <UiButton
            variant="accent"
            size="lg"
            v-if="engineInstalledButNotRunning && !isLinux"
            :disabled="startingEngine"
            @click="startEngine"
          >
            <span v-if="startingEngine" class="ob-spinner"></span>
            <template v-if="isWslc">{{ startingEngine ? 'Connecting...' : 'Retry WSL containers' }}</template>
            <template v-else>{{ startingEngine ? 'Starting Docker...' : 'Start Docker' }}</template>
          </UiButton>

          <UiButton
            variant="quiet"
            size="lg"
            v-if="canSwitchToWslc && engineInstalledButNotRunning"
            :disabled="switching"
            @click="useEngine('wslc')"
          >
            Use WSL containers instead
          </UiButton>

          <UiButton
            variant="primary"
            size="lg"
            v-if="canContinue"
            @click="step = 2"
          >
            Continue &#x2192;
          </UiButton>

          <UiButton
            size="lg"
            v-if="!canContinue"
            disabled
          >
            Continue &#x2192;
          </UiButton>

          <UiButton
            variant="quiet"
            size="lg"
            :disabled="checking"
            @click="check"
          >
            <span v-if="checking" class="ob-spinner"></span>
            {{ checking ? 'Checking...' : '&#x21BB; Refresh checks' }}
          </UiButton>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.ob-viewport {
  /* v3.7.2: menutupi seluruh jendela, supaya sisa layar sebelumnya (mis.
     Settings saat "Run setup again") tidak terlihat di tepi atas. */
  position: fixed;
  inset: 0;
  z-index: 1;
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
  /* Tengah vertikal bila konten lebih pendek dari jendela. `margin: auto`
     (bukan align-items: center) supaya konten yang lebih tinggi tetap bisa
     di-scroll dari atas, tidak terpotong. */
  margin-block: auto;
}

.ob-mark {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-bottom: var(--space-6);
}

.ob-heading {
  font-size: var(--text-xl);
  font-weight: 600;
  color: var(--text);
  margin: 0 0 var(--space-2) 0;
}

.ob-sub {
  font-size: var(--text-md);
  color: var(--muted);
  margin: 0 0 var(--space-6) 0;
}

.ob-steps {
  list-style: none;
  margin: 0 0 var(--space-6);
  padding: 0;
  display: flex;
  gap: var(--space-4);
  font-size: var(--text-sm);
  color: var(--faint);
}
.ob-step { display: inline-flex; align-items: center; gap: var(--space-2); }
.ob-step__n {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 1px solid var(--border-strong);
  font: 500 var(--text-xs) var(--font-mono);
}
.ob-step--on { color: var(--text); }
.ob-step--on .ob-step__n { border-color: var(--accent); color: var(--accent); }
.ob-step--done { color: var(--muted); }
.ob-step--done .ob-step__n { border-color: transparent; background: color-mix(in srgb, var(--green) 20%, transparent); color: var(--green); }

.ob-first { display: flex; flex-direction: column; gap: var(--space-3); padding: var(--space-5); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--surface); }
.ob-first__title { margin: 0; font-size: var(--text-md); font-weight: 600; color: var(--text); }
.ob-first__text { margin: 0; color: var(--muted); font-size: var(--text-sm); }
.ob-first__kinds { margin: 0; color: var(--faint); font-size: var(--text-xs); }

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
  font-size: var(--text-sm);
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
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--amber);
  margin: 0 0 var(--space-2) 0;
}

.ob-instruction__cmd {
  display: block;
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--text);
  background: var(--surface2);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  margin-bottom: var(--space-2);
}

.ob-instruction__hint {
  font-size: var(--text-xs);
  color: var(--muted);
  margin: 0;
}

.ob-note {
  font-size: var(--text-sm);
  color: var(--muted);
  margin: 0 0 var(--space-4) 0;
}

.ob-cta-row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: center;
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
