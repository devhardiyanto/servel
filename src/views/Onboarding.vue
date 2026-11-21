<script setup lang="ts">
// Onboarding tiga langkah (canvas Flow A, artboard A1–A7).
// v3.8.2: satu kartu ringkas seperti canvas. Langkah Engine menampilkan
// WSL containers dan Docker berdampingan dengan hasil pemeriksaannya, plus
// pilihan engine (WSL containers / Docker / Auto). Langkah Runtimes memakai
// judul, log, dan state gagal dari A3–A5.
import { openExternal } from '@/utils/openExternal'
import UiButton from '@/components/ui/UiButton.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiChip from '@/components/ui/UiChip.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import { inject, computed, ref, onMounted, watch, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { SetViewKey } from '../types/navigation'
import { usePrereq } from '@/composables/usePrereq'
import { useTauri } from '@/composables/useTauri'
import { useConfig } from '@/composables/useConfig'
import { useToast } from '@/composables/useToast'
import type { EnginePreference } from '@/types/prereq'
import KovaMark from '@/components/shell/KovaMark.vue'
import AddProjectForm from '@/components/AddProjectForm.vue'
import { open as openFolderDialog } from '@tauri-apps/plugin-dialog'

const setView = inject(SetViewKey)!
const toast = useToast()

// Platform dipakai untuk menampilkan instruksi prasyarat yang sesuai OS.
// Default "windows" (target utama) sampai command resolve.
const { call } = useTauri()
const platform = ref<string>('windows')
onMounted(async () => {
  const p = await call<string>('get_platform')
  if (p) platform.value = p
})
const isLinux = computed<boolean>(() => platform.value === 'linux')
const isWindows = computed<boolean>(() => platform.value === 'windows')

const {
  status,
  checking,
  startingEngine,
  startEngineError,
  canContinue,
  engineInstalledButNotRunning,
  check,
  startEngine,
} = usePrereq()

const isWslc = computed<boolean>(() => status.value?.engine === 'wslc')

const { config: cfg, markOnboarded, setContainerRuntime } = useConfig()
const switching = ref(false)
async function useEngine(kind: EnginePreference): Promise<void> {
  switching.value = true
  try {
    await setContainerRuntime(kind)
    await check()
  } finally {
    switching.value = false
  }
}

function openUrl(url: string): void {
  void openExternal(url)
}

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
    toast.show(`Copied ${text}`)
  } catch {
    /* clipboard ditolak: teks tetap terlihat dan bisa dipilih */
  }
}

const DOCKER_DESKTOP_URL = 'https://www.docker.com/products/docker-desktop/'
const WSL_RELEASES_URL = 'https://github.com/microsoft/WSL/releases'

// --- Langkah 1: engine -------------------------------------------------------

const ENGINE_OPTIONS = [
  { value: 'wslc', label: 'WSL containers' },
  { value: 'docker', label: 'Docker' },
  { value: 'auto', label: 'Auto' },
]
const enginePref = computed<EnginePreference>(() => cfg.value.containerRuntime)
function onEnginePref(v: string): void {
  if (v !== enginePref.value) void useEngine(v as EnginePreference)
}

type RowState = 'checking' | 'ok' | 'warn' | 'off'
interface EngineRow {
  id: 'wslc' | 'docker'
  name: string
  state: RowState
  text: string
  recommended: boolean
  inUse: boolean
}

/** A1/A2: satu baris per engine, dengan hasil pemeriksaannya. */
const engineRows = computed<EngineRow[]>(() => {
  const s = status.value
  const rows: EngineRow[] = []
  if (isWindows.value) {
    let state: RowState = 'checking'
    let text = 'Checking wsl --version…'
    if (s) {
      if (s.wslc_available) {
        const quiet = s.engine === 'wslc' && !s.engine_running
        state = quiet ? 'warn' : 'ok'
        text = quiet ? 'Installed, but not responding' : `Ready${s.wsl_version ? ` · WSL ${s.wsl_version}` : ''}`
      } else if (s.wsl_version) {
        state = 'warn'
        text = `WSL ${s.wsl_version} is too old · needs 3.0.1 or newer`
      } else {
        state = 'off'
        text = 'Not found'
      }
    }
    rows.push({ id: 'wslc', name: 'WSL containers', state, text, recommended: true, inUse: !!s && s.engine === 'wslc' })
  }
  let state: RowState = 'checking'
  let text = 'Checking…'
  if (s) {
    const active = s.engine === 'docker'
    if (active && s.engine_problem === 'permission') {
      state = 'warn'
      text = 'Installed · permission denied on the Docker socket'
    } else if (active && s.engine_running && s.engine_problem === 'compose') {
      state = 'warn'
      text = `Running${s.engine_version ? ` · v${s.engine_version}` : ''} · Compose v2 missing`
    } else if (active && s.engine_running) {
      state = 'ok'
      text = `Running${s.engine_version ? ` · v${s.engine_version}` : ''}`
    } else if (s.docker_available || (active && s.engine_installed)) {
      state = active ? 'warn' : 'ok'
      text = active ? 'Installed, but not running' : 'Installed'
    } else {
      state = 'off'
      text = isWindows.value && s.wslc_available ? 'Not found · not needed on Windows' : 'Not found'
    }
  }
  rows.push({ id: 'docker', name: 'Docker', state, text, recommended: false, inUse: !!s && s.engine === 'docker' })
  return rows
})

/** WSL terlalu lama / belum ada, sementara engine yang dipakai WSL containers. */
const wslTooOld = computed<boolean>(
  () => isWindows.value && !!status.value && isWslc.value && !status.value.engine_installed,
)
const wslTooOldTitle = computed<string>(() => {
  const v = status.value?.wsl_version
  return v ? `WSL ${v} is too old for WSL containers` : 'WSL containers need WSL 3.0.1 or newer'
})

/** v3.12.0: Docker terpasang tetapi user belum di grup `docker` / Compose v2 tidak ada. */
const dockerProblem = computed(() => (status.value?.engine === 'docker' ? status.value.engine_problem ?? null : null))
const DOCKER_GROUP_CMD = 'sudo usermod -aG docker $USER'
const COMPOSE_CMD = 'sudo apt install docker-compose-plugin'
const COMPOSE_DOCS_URL = 'https://docs.docker.com/compose/install/linux/'

/** Docker dipakai tapi belum terpasang. */
const dockerMissing = computed<boolean>(
  () => !!status.value && status.value.engine === 'docker' && !status.value.engine_installed,
)
const dockerInstallUrl = computed<string>(() =>
  isLinux.value ? 'https://docs.docker.com/engine/install/' : DOCKER_DESKTOP_URL,
)

const showElevatedWarning = computed<boolean>(
  () => isWindows.value && isWslc.value && !!status.value?.elevated,
)

// --- Langkah 2: mise --------------------------------------------------------

// v3: mise satu-satunya pengelola versi PHP/Node.
const usesMise = computed<boolean>(() => !!status.value?.mise_installed)

// mise belum ada (v2.11): Windows dipasang satu klik lewat winget; macOS/Linux
// diberi perintah resminya.
const installingMise = ref(false)
const miseInstallMsg = ref('')
const miseInstallErr = ref('')
/** "I'll install it myself": tampilkan perintahnya, lalu Check again / Skip. */
const miseManual = ref(false)

/** v3.8.3: output winget baris per baris saat berjalan (A4). */
const wingetLines = ref<string[]>([])
const wingetProgress = ref('')
const outEl = ref<HTMLElement | null>(null)

async function installMise(): Promise<void> {
  installingMise.value = true
  miseInstallErr.value = ''
  miseInstallMsg.value = ''
  wingetLines.value = []
  wingetProgress.value = ''
  const offLine = await listen<{ line: string; source?: string }>('cmd-output', (e) => {
    if (e.payload.source === 'MISE') wingetLines.value = [...wingetLines.value, e.payload.line].slice(-200)
  })
  const offProgress = await listen<{ text: string }>('mise-install-progress', (e) => {
    wingetProgress.value = e.payload.text
  })
  try {
    miseInstallMsg.value = await invoke<string>('mise_install_winget')
    await check()
  } catch (err) {
    miseInstallErr.value = String(err)
  } finally {
    offLine()
    offProgress()
    wingetProgress.value = ''
    installingMise.value = false
  }
}

const miseInstallCmd = computed<string>(() => {
  if (isWindows.value) return 'winget install jdx.mise'
  if (platform.value === 'macos') return 'brew install mise'
  return 'curl https://mise.run | sh'
})

/** A3 minta pasang · A4 memasang · A5 selesai · gagal · pasang sendiri. */
const misePhase = computed<'ask' | 'running' | 'done' | 'failed' | 'manual'>(() => {
  if (usesMise.value) return 'done'
  if (installingMise.value) return 'running'
  if (miseInstallErr.value) return 'failed'
  if (!isWindows.value || miseManual.value) return 'manual'
  return 'ask'
})
const wingetMissing = computed<boolean>(() => /winget is not available/i.test(miseInstallErr.value))
const miseTitle = computed<string>(() =>
  misePhase.value === 'done' ? 'mise is installed' : 'Install mise to manage PHP, Node, Go and Python',
)
const miseText = computed<string>(() => {
  if (misePhase.value === 'done') {
    const v = status.value?.mise_version
    return `${v ? `mise ${v}. ` : ''}Kova uses it for every runtime version.`
  }
  if (misePhase.value === 'manual') return `Run this in a terminal, then press Check again. You can also continue without it.`
  return 'Kova uses mise for versions. It is not installed yet.'
})
/** Baris log di kartu: perintah winget, output-nya, lalu hasil (A4/A5). */
const miseLog = computed<{ x: string; cmd?: boolean }[]>(() => {
  if (misePhase.value === 'ask' || misePhase.value === 'manual') return []
  if (!installingMise.value && wingetLines.value.length === 0 && !miseInstallMsg.value) return []
  const [first, ...rest] = wingetLines.value
  const lines: { x: string; cmd?: boolean }[] = [{ x: `> ${first ?? miseInstallCmd.value}`, cmd: true }]
  lines.push(...rest.map((x) => ({ x })))
  if (miseInstallMsg.value) lines.push({ x: miseInstallMsg.value })
  return lines
})

watch(() => miseLog.value.length, () => nextTick(() => outEl.value?.scrollTo({ top: outEl.value.scrollHeight })))

// --- Langkah 3: project pertama --------------------------------------------

// v3.4 (canvas Flow A): tiga langkah — Engine, Runtimes, First project.
const step = ref<1 | 2 | 3>(1)
const steps = [
  { n: 1, label: 'Engine' },
  { n: 2, label: 'Runtimes' },
  { n: 3, label: 'First project' },
] as const

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
</script>

<template>
  <div class="ob-viewport">
    <div class="ob-main">
      <section class="ob-card" aria-labelledby="ob-title">
        <header class="ob-head">
          <KovaMark :size="20" />
          <h1 id="ob-title" class="ob-brand">Welcome to Kova</h1>
          <span class="ob-sp"></span>
          <ol class="ob-steps" aria-label="Setup steps">
            <template v-for="(st, i) in steps" :key="st.n">
              <li
                class="ob-step"
                :class="{ 'ob-step--on': step === st.n, 'ob-step--done': step > st.n }"
                :aria-current="step === st.n ? 'step' : undefined"
              >
                <span class="ob-step__n">{{ step > st.n ? '✓' : st.n }}</span>{{ st.label }}
              </li>
              <li v-if="i < steps.length - 1" class="ob-step__ln" aria-hidden="true"></li>
            </template>
          </ol>
        </header>

        <!-- Langkah 1: engine (A1, A2) -->
        <template v-if="step === 1">
          <div class="ob-intro">
            <h2 class="ob-title">Choose where services run</h2>
            <p class="ob-text">MySQL, Redis and the other services run in containers. Kova checks what this PC has.</p>
          </div>

          <ul class="ob-rows" :aria-busy="status === null" aria-label="Container engines">
            <li v-for="r in engineRows" :key="r.id" class="ob-row" :class="{ 'ob-row--use': r.inUse }">
              <span class="ob-row__ico" aria-hidden="true">
                <span v-if="r.state === 'checking'" class="ob-spinner ob-spinner--amber"></span>
                <svg v-else-if="r.state === 'ok'" class="ob-i ob-i--ok" viewBox="0 0 16 16"><path d="M3.5 8.5l3 3 6-7" /></svg>
                <svg v-else-if="r.state === 'warn'" class="ob-i ob-i--warn" viewBox="0 0 16 16"><path d="M8 2.5L14 13H2zM8 7v2.5M8 11.2h.01" /></svg>
                <span v-else class="ob-ring"></span>
              </span>
              <span class="ob-row__lbl">
                <b>{{ r.name }}</b>
                <span>{{ r.text }}</span>
              </span>
              <UiChip v-if="r.inUse && status" tone="neutral">in use</UiChip>
              <UiChip v-if="r.recommended" tone="accent">recommended on Windows</UiChip>
            </li>
          </ul>

          <UiBanner v-if="wslTooOld" tone="warn" :title="wslTooOldTitle">
            Update WSL, then check again. Nothing else on this PC changes.
            <code class="ob-code">wsl --update</code>
            <template #actions>
              <UiButton @click="copy('wsl --update')">Copy</UiButton>
              <UiButton variant="quiet" @click="openUrl(WSL_RELEASES_URL)">WSL releases ↗</UiButton>
            </template>
          </UiBanner>

          <UiBanner v-else-if="dockerMissing" tone="info" title="Docker is not installed">
            <template v-if="isWindows && status?.wslc_available">WSL containers are on this PC and run your services without Docker Desktop.</template>
            <template v-else-if="isLinux">Install Docker Engine, then check again.</template>
            <template v-else>Install Docker Desktop, then check again.</template>
            <template #actions>
              <UiButton v-if="isWindows && status?.wslc_available" variant="accent" :disabled="switching" @click="useEngine('wslc')">Use WSL containers</UiButton>
              <UiButton variant="quiet" @click="openUrl(dockerInstallUrl)">{{ isLinux ? 'Docker Engine ↗' : 'Docker Desktop ↗' }}</UiButton>
            </template>
          </UiBanner>

          <UiBanner
            v-else-if="dockerProblem === 'permission'"
            tone="warn"
            title="Docker is running, but Kova is not allowed to use it"
          >
            Your user is not in the docker group, so the Docker socket refuses the connection. Add it, then log out and back in
            (or restart) and check again.
            <code class="ob-code">{{ DOCKER_GROUP_CMD }}</code>
            <template #actions>
              <UiButton @click="copy(DOCKER_GROUP_CMD)">Copy</UiButton>
            </template>
          </UiBanner>

          <UiBanner
            v-else-if="dockerProblem === 'compose'"
            tone="warn"
            title="Docker Compose v2 is missing"
          >
            Kova starts services with docker compose. Install the Compose plugin, then check again. On Debian and Ubuntu:
            <code class="ob-code">{{ COMPOSE_CMD }}</code>
            <template #actions>
              <UiButton @click="copy(COMPOSE_CMD)">Copy</UiButton>
              <UiButton variant="quiet" @click="openUrl(COMPOSE_DOCS_URL)">Install guide ↗</UiButton>
            </template>
          </UiBanner>

          <UiBanner
            v-else-if="engineInstalledButNotRunning && isLinux"
            tone="warn"
            title="Docker is installed but not running"
          >
            Start it, then check again.
            <code class="ob-code">sudo systemctl start docker</code>
            <template #actions>
              <UiButton @click="copy('sudo systemctl start docker')">Copy</UiButton>
            </template>
          </UiBanner>

          <UiBanner
            v-else-if="engineInstalledButNotRunning"
            tone="warn"
            :title="isWslc ? 'WSL containers are installed but not responding' : 'Docker is installed but not running'"
          >
            {{ isWslc ? 'The WSL session can take a few seconds to start.' : 'Start Docker Desktop, then check again.' }}
            <template #actions>
              <UiButton variant="accent" :loading="startingEngine" @click="startEngine">
                {{ isWslc ? (startingEngine ? 'Connecting…' : 'Retry WSL containers') : (startingEngine ? 'Starting Docker…' : 'Start Docker') }}
              </UiButton>
              <UiButton v-if="!isWslc && isWindows && status?.wslc_available" variant="quiet" :disabled="switching" @click="useEngine('wslc')">
                Use WSL containers instead
              </UiButton>
            </template>
          </UiBanner>

          <UiBanner v-if="showElevatedWarning" tone="warn" title="Kova is running as Administrator">
            WSL containers keep a separate session per elevation, so containers created from a normal window will not
            appear here. Restart Kova without "Run as administrator".
          </UiBanner>

          <UiBanner v-if="startEngineError && !isLinux" tone="err" title="The engine did not start">
            {{ startEngineError }}
          </UiBanner>

          <div class="ob-foot">
            <template v-if="isWindows">
              <span class="ob-label">Engine</span>
              <UiSegmented
                :model-value="enginePref"
                :options="ENGINE_OPTIONS.map((o) => ({ ...o, disabled: switching }))"
                label="Container engine"
                @update:model-value="onEnginePref"
              />
            </template>
            <span class="ob-sp"></span>
            <UiButton variant="quiet" size="md" :loading="checking && status !== null" :disabled="checking" @click="check">
              Check again
            </UiButton>
            <UiButton variant="primary" size="md" :disabled="!canContinue" @click="step = 2">Continue</UiButton>
          </div>
        </template>

        <!-- Langkah 2: mise (A3, A4, A5) -->
        <template v-else-if="step === 2">
          <div class="ob-intro">
            <h2 class="ob-title">{{ miseTitle }}</h2>
            <p class="ob-text">{{ miseText }}</p>
          </div>

          <div v-if="misePhase === 'manual'" class="ob-out">
            <div class="ob-ol ob-ol--cmd">
              <span class="ob-ol__x">{{ miseInstallCmd }}</span>
              <UiButton variant="quiet" @click="copy(miseInstallCmd)">Copy</UiButton>
            </div>
          </div>

          <div v-if="miseLog.length > 0" ref="outEl" class="ob-out" tabindex="0" aria-label="winget output">
            <div v-for="(l, i) in miseLog" :key="i" class="ob-ol" :class="{ 'ob-ol--cmd': l.cmd }"><span class="ob-ol__x">{{ l.x }}</span></div>
          </div>

          <div v-if="misePhase === 'running'" class="ob-progw">
            <div class="ob-prog" role="progressbar" aria-label="Installing mise" :aria-valuetext="wingetProgress || undefined"><span></span></div>
            <span v-if="wingetProgress" class="ob-text ob-text--faint ob-mono">{{ wingetProgress }}</span>
          </div>

          <UiBanner
            v-if="misePhase === 'failed'"
            tone="err"
            :title="wingetMissing ? 'winget isn\'t available on this PC' : 'mise was not installed'"
          >
            <template v-if="wingetMissing">Install mise yourself with one of the commands on mise.jdx.dev, then press Check again.</template>
            <template v-else>{{ miseInstallErr }}</template>
            <template #actions>
              <UiButton variant="quiet" @click="openUrl('https://mise.jdx.dev/getting-started.html')">mise.jdx.dev ↗</UiButton>
            </template>
          </UiBanner>

          <div class="ob-foot">
            <template v-if="misePhase === 'ask'">
              <UiButton variant="primary" size="md" @click="installMise">Install mise</UiButton>
              <UiButton variant="quiet" size="md" @click="miseManual = true">I'll install it myself</UiButton>
            </template>
            <UiButton v-else-if="misePhase === 'running'" variant="primary" size="md" loading disabled>Installing…</UiButton>
            <template v-else-if="misePhase === 'failed' || misePhase === 'manual'">
              <UiButton variant="primary" size="md" :loading="checking" @click="check">Check again</UiButton>
              <UiButton variant="quiet" size="md" @click="afterRuntimes">Skip for now</UiButton>
            </template>
            <span class="ob-sp"></span>
            <UiButton variant="quiet" size="md" @click="step = 1">Back</UiButton>
            <UiButton v-if="misePhase === 'done'" variant="primary" size="md" @click="afterRuntimes">Continue</UiButton>
          </div>
        </template>

        <!-- Langkah 3: project pertama (A6, A7) -->
        <template v-else>
          <div class="ob-intro">
            <h2 class="ob-title">Add your first project</h2>
            <p v-if="!firstPath" class="ob-text">Pick a folder. Kova detects how to run it and gives it a fixed localhost port.</p>
          </div>

          <div v-if="!firstPath" class="ob-drop">
            <span class="ob-text ob-text--faint">Laravel, WordPress, PHP, Django, FastAPI, Flask, Vite, Next, Nuxt, Angular, Node, Go</span>
            <UiButton variant="primary" size="md" @click="chooseFirstFolder">Choose folder</UiButton>
          </div>
          <UiBanner v-if="pickErr" tone="err" title="The folder could not be opened">{{ pickErr }}</UiBanner>
          <AddProjectForm v-if="firstPath" ref="firstForm" :key="firstPath" :path="firstPath" compact />

          <div class="ob-foot">
            <UiButton variant="quiet" size="md" @click="firstPath ? (firstPath = '') : (step = 2)">Back</UiButton>
            <span class="ob-sp"></span>
            <UiButton v-if="!firstPath" variant="quiet" size="md" @click="finish">Skip for now</UiButton>
            <template v-else>
              <UiButton size="md" :disabled="!firstForm?.ready || firstForm?.saving" @click="addFirst(false)">Add</UiButton>
              <UiButton
                variant="primary"
                size="md"
                :disabled="!firstForm?.ready || !firstForm?.canStart || firstForm?.saving"
                @click="addFirst(true)"
              >Add &amp; start</UiButton>
            </template>
          </div>
        </template>
      </section>
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
     terjadi di .ob-main. */
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
  padding: var(--space-8) var(--space-4);
}

/* Kartu ringkas (canvas .k-obcard). `margin-block: auto` menengahkan secara
   vertikal tanpa memotong kartu yang lebih tinggi dari jendela. */
.ob-card {
  width: 100%;
  max-width: 600px;
  margin-block: auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  padding: var(--space-6);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
}

.ob-head { display: flex; align-items: center; gap: var(--space-3); flex-wrap: wrap; }
.ob-brand { margin: 0; font-size: var(--text-md); font-weight: 600; }
.ob-sp { flex: 1; }

.ob-steps {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--muted);
}
.ob-step { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
.ob-step__ln { width: 20px; height: 1px; background: var(--border-strong); }
.ob-step__n {
  display: inline-grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 1px solid var(--border-strong);
  font: 500 var(--text-xs) var(--font-mono);
}
.ob-step--on { color: var(--text); }
.ob-step--on .ob-step__n { border-color: var(--accent); background: var(--accent); color: var(--bg); }
.ob-step--done .ob-step__n { border-color: transparent; background: color-mix(in srgb, var(--green) 20%, transparent); color: var(--green); }

.ob-intro { display: flex; flex-direction: column; gap: var(--space-1); }
.ob-title { margin: 0; font-size: var(--text-xl); font-weight: 600; color: var(--text); }
.ob-text { margin: 0; font-size: var(--text-sm); color: var(--muted); line-height: 1.5; }
.ob-text--faint { color: var(--faint); font-size: var(--text-xs); }

.ob-rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-2); }
.ob-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg);
}
.ob-row--use { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
.ob-row__ico { display: inline-grid; place-items: center; width: 18px; flex: none; }
.ob-row__lbl { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; font-size: var(--text-sm); }
.ob-row__lbl b { font-weight: 600; color: var(--text); }
.ob-row__lbl span { color: var(--muted); font-size: var(--text-xs); }

.ob-i { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
.ob-i--ok { color: var(--green); }
.ob-i--warn { color: var(--amber); }
.ob-ring { width: 10px; height: 10px; border-radius: 50%; border: 1.5px solid var(--faint); }

.ob-code {
  display: block;
  margin-top: var(--space-2);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  color: var(--text);
}

.ob-foot { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
.ob-label { font-size: var(--text-xs); color: var(--muted); margin-right: var(--space-1); }

.ob-out {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  font: var(--text-xs) / 1.6 var(--font-mono);
  color: var(--text);
  max-height: 180px;
  overflow-y: auto;
}
.ob-ol { display: flex; align-items: center; gap: var(--space-2); }
.ob-ol__x { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.ob-ol--cmd { color: var(--muted); }

/* Progres tanpa angka: winget tidak memberi persentase yang bisa dibaca. */
.ob-progw { display: flex; flex-direction: column; gap: var(--space-1); }
.ob-mono { font-family: var(--font-mono); }
.ob-prog { position: relative; height: 3px; border-radius: 2px; background: var(--surface2); overflow: hidden; }
.ob-prog span { position: absolute; inset: 0 auto 0 0; width: 35%; background: var(--accent); border-radius: 2px; animation: ob-slide 1.4s ease-in-out infinite; }
@keyframes ob-slide { from { transform: translateX(-100%); } to { transform: translateX(290%); } }

.ob-drop {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-6) var(--space-4);
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-md);
  text-align: center;
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
.ob-spinner--amber { color: var(--amber); }

@keyframes spin {
  to { transform: rotate(360deg); }
}

@media (prefers-reduced-motion: reduce) {
  .ob-prog span, .ob-spinner { animation-duration: 3s; }
}
</style>
