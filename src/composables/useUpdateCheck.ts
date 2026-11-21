import { openExternal } from '@/utils/openExternal'
import { ref, computed } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

const REPO = 'devhardiyanto/servel'
const RELEASE_PAGE = `https://github.com/${REPO}/releases/latest`

/**
 * Bandingkan dua versi semver sederhana. Return >0 bila `a` lebih baru dari `b`.
 * Prerelease (`1.1.0-beta.1`) dianggap LEBIH LAMA dari rilis penuh base yang sama
 * (`1.1.0`), sesuai spec semver. Cukup untuk major.minor.patch[-pre].
 */
export function compareSemver(a: string, b: string): number {
  const parse = (v: string) => {
    const [core, pre] = v.replace(/^v/, '').split('-')
    const nums = core.split('.').map((n) => parseInt(n, 10) || 0)
    return { nums, pre: pre ?? '' }
  }
  const pa = parse(a)
  const pb = parse(b)
  for (let i = 0; i < 3; i++) {
    const d = (pa.nums[i] ?? 0) - (pb.nums[i] ?? 0)
    if (d !== 0) return d > 0 ? 1 : -1
  }
  // Base sama: tanpa prerelease > dengan prerelease.
  if (pa.pre === pb.pre) return 0
  if (pa.pre === '') return 1
  if (pb.pre === '') return -1
  return pa.pre > pb.pre ? 1 : -1
}

// State singleton — cek sekali saat launch, dibagi ke semua konsumen.
const currentVersion = ref<string>('')
const latestVersion = ref<string>('')
const updateAvailable = ref(false)
const checking = ref(false)
const lastChecked = ref<number | null>(null)
/** v3.8.2: pengecekan terakhir gagal (jaringan, cache GitHub, latest.json). */
const checkError = ref('')
// In-app updater (v2.1). `canInstall` = updater bawaan menemukan versi yang
// sama dan bisa memasangnya; bila false, banner hanya menawarkan halaman rilis.
const canInstall = ref(false)
const installing = ref(false)
const installPercent = ref<number | null>(null)
const installError = ref('')
// v3.4: banner global. Unduh dulu, lalu pengguna memilih kapan restart.
export type UpdatePhase = 'idle' | 'available' | 'downloading' | 'ready' | 'error'
const phase = ref<UpdatePhase>('idle')
const downloaded = ref(0)
const totalBytes = ref<number | null>(null)
const readyVersion = ref('')
const DISMISS_KEY = 'kova.update.dismissed'
const dismissedVersion = ref<string>(readDismissed())
const bannerHidden = ref(false)
/** v3.13.0 (canvas F3): Later = pasang saat Kova ditutup. */
const installOnQuit = ref(false)

async function later(): Promise<void> {
  bannerHidden.value = true
  try {
    installOnQuit.value = await invoke<boolean>('update_install_on_quit', { enabled: true })
  } catch (err) {
    console.warn('[update] install on quit not available:', err)
    installOnQuit.value = false
  }
}

/**
 * v3.13.0 (canvas F4): versi yang terakhir dijalankan disimpan; bila versi
 * sekarang lebih baru, Kova baru saja diperbarui → tawarkan What's new.
 */
const LAST_RUN_KEY = 'kova.lastRunVersion'
async function consumeJustUpdated(): Promise<{ from: string; to: string; url: string } | null> {
  let current = ''
  try {
    current = await getVersion()
  } catch {
    return null
  }
  let last = ''
  try {
    last = localStorage.getItem(LAST_RUN_KEY) ?? ''
    localStorage.setItem(LAST_RUN_KEY, current)
  } catch {
    return null
  }
  if (!last || compareSemver(current, last) <= 0) return null
  return { from: last, to: current, url: `https://github.com/${REPO}/releases/tag/v${current}` }
}

function readDismissed(): string {
  try {
    return localStorage.getItem(DISMISS_KEY) ?? ''
  } catch {
    return ''
  }
}

interface UpdateInfo {
  version: string
  currentVersion: string
  notes: string | null
}

interface UpdateProgress {
  downloaded: number
  total: number | null
  percent: number | null
}

/** Tanya updater bawaan. Gagal (build dev, release tanpa latest.json) = false. */
async function probeInstaller(): Promise<void> {
  try {
    const info = await invoke<UpdateInfo | null>('update_check')
    canInstall.value = info !== null
    if (info) {
      latestVersion.value = info.version
      updateAvailable.value = true
      if (phase.value === 'idle') phase.value = 'available'
    }
  } catch (err) {
    // v3.8.2: dulu hanya ke console, sehingga pengecekan yang gagal tampil
    // sebagai "You have the latest version".
    console.warn('[update-check] could not check:', err)
    canInstall.value = false
    checkError.value = String(err).replace(/^could not check for updates:\s*/i, '')
  }
}

async function runInstall(): Promise<void> {
  if (installing.value) return
  installing.value = true
  installError.value = ''
  installPercent.value = null
  const unlisten = await listen<UpdateProgress>('update-progress', (e) => {
    installPercent.value = e.payload.percent
  })
  try {
    // Sukses = aplikasi restart, jadi baris setelah ini biasanya tak tercapai.
    await invoke('update_install')
  } catch (err) {
    installError.value = String(err)
    canInstall.value = false
  } finally {
    unlisten()
    installing.value = false
  }
}

/** Unduh dan verifikasi; banner berpindah ke "ready" bila berhasil. */
async function runDownload(): Promise<void> {
  if (phase.value === 'downloading') return
  phase.value = 'downloading'
  bannerHidden.value = false
  installError.value = ''
  installPercent.value = null
  downloaded.value = 0
  totalBytes.value = null
  const unlisten = await listen<UpdateProgress>('update-progress', (e) => {
    installPercent.value = e.payload.percent
    downloaded.value = e.payload.downloaded
    totalBytes.value = e.payload.total
  })
  try {
    readyVersion.value = await invoke<string>('update_download')
    phase.value = 'ready'
  } catch (err) {
    installError.value = String(err)
    phase.value = 'error'
  } finally {
    unlisten()
  }
}

/** Pasang yang sudah diunduh lalu restart. */
async function runApply(): Promise<void> {
  installing.value = true
  try {
    await invoke('update_apply')
  } catch (err) {
    installError.value = String(err)
    phase.value = 'error'
  } finally {
    installing.value = false
  }
}

function dismiss(): void {
  dismissedVersion.value = latestVersion.value
  try {
    localStorage.setItem(DISMISS_KEY, latestVersion.value)
  } catch {
    /* hanya untuk sesi ini */
  }
}

async function runCheck(): Promise<void> {
  if (checking.value) return
  checking.value = true
  try {
    if (!currentVersion.value) {
      currentVersion.value = await getVersion()
    }
    // v3: hanya updater bawaan (latest.json bertanda tangan). Cadangan lewat
    // GitHub API dihapus: ia bisa menawarkan versi yang tidak bisa dipasang.
    updateAvailable.value = false
    checkError.value = ''
    await probeInstaller()
  } catch (err) {
    console.error('[update-check]', err)
  } finally {
    checking.value = false
    lastChecked.value = Date.now()
  }
}

// v3.8.2: Kova yang dibiarkan terbuka tidak tahu rilis baru sampai dibuka
// ulang. Periksa lagi setiap 6 jam, dan saat jendela kembali difokuskan bila
// pemeriksaan terakhir lebih dari 30 menit lalu.
const PERIODIC_MS = 6 * 60 * 60 * 1000
const FOCUS_MIN_MS = 30 * 60 * 1000
let autoTimer: ReturnType<typeof setInterval> | null = null
function maybeCheck(minAgeMs: number): void {
  // Jangan mengganggu unduhan atau update yang menunggu restart.
  if (phase.value === 'downloading' || phase.value === 'ready') return
  if (lastChecked.value !== null && Date.now() - lastChecked.value < minAgeMs) return
  void runCheck()
}
function startAutoCheck(): void {
  if (autoTimer) return
  autoTimer = setInterval(() => maybeCheck(PERIODIC_MS - 60_000), PERIODIC_MS)
  window.addEventListener('focus', () => maybeCheck(FOCUS_MIN_MS))
}

/** Banner update di atas jendela sedang tampil (v3.6.1: About tidak menduplikasinya). */
const bannerVisible = computed(() => {
  if (bannerHidden.value) return false
  if (phase.value === 'available') return dismissedVersion.value !== latestVersion.value
  return phase.value !== 'idle'
})

export function useUpdateCheck() {
  function openReleasePage(): void {
    void openExternal(RELEASE_PAGE)
  }

  return {
    currentVersion,
    latestVersion,
    updateAvailable,
    checking,
    lastChecked,
    checkError,
    canInstall,
    installing,
    installPercent,
    installError,
    phase,
    downloaded,
    totalBytes,
    readyVersion,
    dismissedVersion,
    bannerHidden,
    bannerVisible,
    download: runDownload,
    apply: runApply,
    dismiss,
    hideBanner: () => (bannerHidden.value = true),
    later,
    installOnQuit,
    consumeJustUpdated,
    check: runCheck,
    startAutoCheck,
    install: runInstall,
    openReleasePage,
  }
}
