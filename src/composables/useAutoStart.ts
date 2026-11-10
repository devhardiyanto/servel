import { invoke } from '@tauri-apps/api/core'
import { useConfig } from './useConfig'
import { useServices } from './useServices'
import { useLogs } from './useLogs'
import { ENGINE_LABEL, type PrereqStatus } from '@/types/prereq'

// Module-level guard — fire sekali per app lifetime (bukan per component mount).
let didAutoStart = false

function nowTs(): string {
  return new Date().toTimeString().slice(0, 8)
}

export function useAutoStart() {
  async function runAutoStartOnce(): Promise<void> {
    if (didAutoStart) return
    didAutoStart = true

    const cfgApi = useConfig()
    const services = useServices()
    const { push: pushLog } = useLogs('ENV')

    // Pastikan services definitions + uiState siap sebelum kita panggil start()
    // (load idempotent via `initialized` flag internal).
    await services.load()

    // Pastikan config sudah loaded (load idempotent via `loaded.value` flag,
    // tapi load() di useConfig tidak memeriksa flag — jadi cek manual).
    if (!cfgApi.loaded.value) {
      await cfgApi.load()
    }

    const cfg = cfgApi.config.value

    if (!cfg.autoStart) {
      pushLog({ ts: nowTs(), src: 'ENV', text: 'auto-start: disabled' })
      return
    }

    if (cfg.selectedServiceIds.length === 0) {
      pushLog({ ts: nowTs(), src: 'ENV', text: 'auto-start: no saved selection' })
      return
    }

    let engineRunning = false
    let engineLabel = 'container engine'
    try {
      const prereqResult = await invoke<PrereqStatus>('check_prerequisites')
      engineRunning = prereqResult.engine_running
      engineLabel = ENGINE_LABEL[prereqResult.engine]
    } catch {
      engineRunning = false
    }

    if (!engineRunning) {
      pushLog({ ts: nowTs(), src: 'ENV', text: `auto-start: ${engineLabel} not ready — skipped` })
      return
    }

    await services.start(cfg.selectedServiceIds)
    pushLog({ ts: nowTs(), src: 'ENV', text: `auto-started: ${cfg.selectedServiceIds.join(', ')}` })
  }

  return { runAutoStartOnce }
}
