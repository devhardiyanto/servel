import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTauri } from './useTauri'

// Status binary Caddy (mirror `ProxyBinaryStatus` Rust).
export interface ProxyBinaryStatus {
  installed: boolean
  version: string | null
  expected_version: string
  path: string
}

// Status proxy (mirror `ProxyStatus` Rust).
export interface ProxyStatus {
  running: boolean
  https: boolean
  routed_sites: number
  cert_installed: boolean
}

// Progres unduhan binary (mirror `DownloadProgress` Rust). `total` 0 = tak diketahui.
export interface ProxyDownloadProgress {
  downloaded: number
  total: number
}

export interface PortInUse {
  port: number
}

export function useProxy() {
  const { on } = useTauri()

  const status = ref<ProxyStatus | null>(null)
  const binary = ref<ProxyBinaryStatus | null>(null)
  const progress = ref<ProxyDownloadProgress | null>(null)

  // Backend meng-emit setiap kali status berubah (start/stop/reload/cert), jadi
  // UI tak perlu polling.
  void on<ProxyStatus>('proxy-status-changed', (payload) => {
    status.value = payload
  })

  void on<ProxyDownloadProgress>('proxy-binary-progress', (payload) => {
    progress.value = payload
  })

  async function refresh(): Promise<void> {
    const [s, b] = await Promise.all([
      invoke<ProxyStatus>('proxy_status').catch(() => null),
      invoke<ProxyBinaryStatus>('proxy_binary_status').catch(() => null),
    ])
    status.value = s
    binary.value = b
  }

  // Semua aksi di bawah THROWS saat gagal — pesan error dari backend sudah
  // Bahasa Indonesia dan actionable, jadi UI menampilkannya apa adanya.
  async function installBinary(): Promise<void> {
    progress.value = null
    binary.value = await invoke<ProxyBinaryStatus>('proxy_binary_install')
    progress.value = null
    await refresh()
  }

  async function checkPorts(): Promise<PortInUse[]> {
    return await invoke<PortInUse[]>('proxy_check_ports')
  }

  async function start(): Promise<void> {
    status.value = await invoke<ProxyStatus>('proxy_start')
  }

  async function stop(): Promise<void> {
    status.value = await invoke<ProxyStatus>('proxy_stop')
  }

  // Terapkan perubahan site ke proxy yang sedang jalan. Aman dipanggil saat
  // proxy mati — backend mengabaikannya.
  async function reload(): Promise<void> {
    status.value = await invoke<ProxyStatus>('proxy_reload')
  }

  async function installCert(): Promise<void> {
    status.value = await invoke<ProxyStatus>('proxy_install_cert')
  }

  return {
    status,
    binary,
    progress,
    refresh,
    installBinary,
    checkPorts,
    start,
    stop,
    reload,
    installCert,
  }
}
