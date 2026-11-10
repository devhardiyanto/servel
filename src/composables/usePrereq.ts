import { ref, computed, onMounted } from 'vue'
// start_engine di-invoke langsung (bukan via call()) agar string error dari Rust bisa di-catch
import { invoke } from '@tauri-apps/api/core'
import { useTauri } from './useTauri'
import { ENGINE_LABEL, type PrereqStatus } from '@/types/prereq'

export function usePrereq() {
  const { call } = useTauri()

  const status = ref<PrereqStatus | null>(null)
  const checking = ref(false)
  const startingEngine = ref(false)
  const startEngineError = ref<string | null>(null)

  const engineReady = computed<boolean>(
    () => !!status.value && status.value.engine_installed && status.value.engine_running,
  )

  const allReady = computed<boolean>(() => {
    if (!status.value) return false
    const s = status.value
    return engineReady.value && s.phpvm_installed && s.fnm_installed
  })

  /** Engine container cukup untuk masuk Dashboard; phpvm/fnm opsional
   *  (bisa digantikan mise, yang integrasinya menyusul di v1.7). */
  const canContinue = computed<boolean>(() => engineReady.value)

  const engineInstalledButNotRunning = computed<boolean>(() => {
    if (!status.value) return false
    return status.value.engine_installed && !status.value.engine_running
  })

  const engineLabel = computed<string>(() => ENGINE_LABEL[status.value?.engine ?? 'docker'])

  /** Engine aktif Docker, tapi WSLC tersedia → tawarkan pindah. */
  const canSwitchToWslc = computed<boolean>(
    () => !!status.value && status.value.engine === 'docker' && status.value.wslc_available,
  )

  async function check(): Promise<void> {
    checking.value = true
    const result = await call<PrereqStatus>('check_prerequisites')
    if (result !== null) {
      status.value = result
    }
    checking.value = false
  }

  async function startEngine(): Promise<void> {
    startingEngine.value = true
    startEngineError.value = null

    try {
      await invoke<void>('start_engine')
    } catch (err) {
      startEngineError.value = typeof err === 'string' ? err : String(err)
      startingEngine.value = false
      return
    }

    // Poll check_prerequisites setiap 3 detik sampai engine running atau timeout 60s
    const pollInterval = 3000
    const timeoutMs = 60000
    const started = Date.now()

    await new Promise<void>((resolve) => {
      const poll = async () => {
        await check()
        if (status.value?.engine_running === true) {
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

    startingEngine.value = false
  }

  onMounted(check)

  return {
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
  }
}
