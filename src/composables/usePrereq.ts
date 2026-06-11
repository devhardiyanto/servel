import { ref, computed, onMounted } from 'vue'
// start_docker di-invoke langsung (bukan via call()) agar string error dari Rust bisa di-catch
import { invoke } from '@tauri-apps/api/core'
import { useTauri } from './useTauri'
import type { PrereqStatus } from '@/types/prereq'

export function usePrereq() {
  const { call } = useTauri()

  const status = ref<PrereqStatus | null>(null)
  const checking = ref(false)
  const startingDocker = ref(false)
  const startDockerError = ref<string | null>(null)

  const allReady = computed<boolean>(() => {
    if (!status.value) return false
    const s = status.value
    return s.docker_installed && s.docker_running && s.phpvm_installed && s.fnm_installed
  })

  const dockerInstalledButNotRunning = computed<boolean>(() => {
    if (!status.value) return false
    return status.value.docker_installed && !status.value.docker_running
  })

  async function check(): Promise<void> {
    checking.value = true
    const result = await call<PrereqStatus>('check_prerequisites')
    if (result !== null) {
      status.value = result
    }
    checking.value = false
  }

  async function startDocker(): Promise<void> {
    startingDocker.value = true
    startDockerError.value = null

    try {
      await invoke<void>('start_docker')
    } catch (err) {
      startDockerError.value = typeof err === 'string' ? err : String(err)
      startingDocker.value = false
      return
    }

    // Poll check_prerequisites setiap 3 detik sampai docker_running atau timeout 60s
    const pollInterval = 3000
    const timeoutMs = 60000
    const started = Date.now()

    await new Promise<void>((resolve) => {
      const poll = async () => {
        await check()
        if (status.value?.docker_running === true) {
          resolve()
          return
        }
        if (Date.now() - started >= timeoutMs) {
          resolve()
          return
        }
        setTimeout(poll, pollInterval)
      }
      setTimeout(poll, pollInterval)
    })

    startingDocker.value = false
  }

  onMounted(check)

  return {
    status,
    checking,
    startingDocker,
    startDockerError,
    allReady,
    dockerInstalledButNotRunning,
    check,
    startDocker,
  }
}
