import { ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'

const REPO = 'devhardiyanto/servel'
const RELEASES_URL = `https://api.github.com/repos/${REPO}/releases/latest`
const RELEASE_PAGE = `https://github.com/${REPO}/releases/latest`

interface GithubRelease {
  tag_name: string
  html_url: string
}

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

async function runCheck(): Promise<void> {
  if (checking.value) return
  checking.value = true
  try {
    if (!currentVersion.value) {
      currentVersion.value = await getVersion()
    }
    const res = await fetch(RELEASES_URL, {
      headers: { Accept: 'application/vnd.github+json' },
    })
    // 404 = belum ada rilis stable (semua masih prerelease) → tak ada update.
    if (!res.ok) {
      updateAvailable.value = false
      return
    }
    const data = (await res.json()) as GithubRelease
    latestVersion.value = data.tag_name.replace(/^v/, '')
    updateAvailable.value = compareSemver(latestVersion.value, currentVersion.value) > 0
  } catch (err) {
    console.error('[update-check]', err)
  } finally {
    checking.value = false
    lastChecked.value = Date.now()
  }
}

export function useUpdateCheck() {
  function openReleasePage(): void {
    window.open(RELEASE_PAGE, '_blank', 'noopener')
  }

  return {
    currentVersion,
    latestVersion,
    updateAvailable,
    checking,
    lastChecked,
    check: runCheck,
    openReleasePage,
  }
}
