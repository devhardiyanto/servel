// Dumps (Experimental, spike AQ): dump()/dd() dari proses PHP yang
// dijalankan Kova. Saklar disimpan backend (file penanda di folder data),
// supaya project yang auto-start sebelum UI siap ikut terbaca.
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Dump {
  type: 'dump'
  text: string
  cut: boolean
  time: number
  file: string | null
  line: number | null
  name: string | null
  request: { method: string | null; uri: string | null; id: string | null } | null
  command: string | null
  client: number
}

export interface DumpsStatus {
  enabled: boolean
  supported: boolean
  state: 'off' | 'starting' | 'listening' | 'failed'
  port: number | null
  error: string | null
  php: string | null
  varDumper: string | null
  count: number
}

export const dumpsEnabled = ref(false)
let loaded = false

export async function loadDumpsEnabled(): Promise<void> {
  if (loaded) return
  loaded = true
  try {
    dumpsEnabled.value = await invoke<boolean>('dumps_enabled')
  } catch {
    dumpsEnabled.value = false
  }
}

export async function setDumpsEnabled(on: boolean): Promise<void> {
  await invoke('dumps_set_enabled', { on })
  dumpsEnabled.value = on
}
