import { computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useConfig, type Site, type SiteTarget } from './useConfig'

// Entri hosts hasil proyeksi (mirror `HostEntry` di Rust).
export interface HostEntry {
  ip: string
  domain: string
}

// Diff blok hosts lama→baru untuk modal konfirmasi (mirror `DiffResult` Rust).
export interface DiffResult {
  added: string[]
  removed: string[]
  unchanged: string[]
}

// Status sinkronisasi Sites (mirror `SitesStatus` Rust).
export interface SitesStatus {
  inSync: boolean
  diff: DiffResult
  backups: string[]
}

// Validasi IPv4 sederhana (O2: custom IP diizinkan). 4 oktet 0–255.
export function isValidIpv4(ip: string): boolean {
  const parts = ip.trim().split('.')
  if (parts.length !== 4) return false
  return parts.every((p) => /^\d{1,3}$/.test(p) && Number(p) <= 255)
}

// Validasi domain lokal ringan: label alnum/hyphen, minimal 1 titik disarankan
// tapi tidak diwajibkan (mis. `myapp.test`). Tolak spasi & karakter aneh.
export function isValidDomain(domain: string): boolean {
  const d = domain.trim()
  if (d.length === 0 || d.length > 253) return false
  return /^[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*$/.test(d)
}

// Port tujuan proxy: 1–65535. String kosong = site hosts-only (tanpa proxy).
export function parsePort(input: string): SiteTarget | null {
  const t = input.trim()
  if (t === '') return null
  if (!/^\d{1,5}$/.test(t)) return null
  const value = Number(t)
  if (value < 1 || value > 65535) return null
  return { kind: 'port', value }
}

export function useSites() {
  const { config, scheduleSave } = useConfig()

  const sites = computed<Site[]>(() => config.value.sites)

  function addSite(domain: string, ip = '127.0.0.1', target: SiteTarget | null = null): string {
    const id = crypto.randomUUID()
    config.value.sites.push({ id, domain: domain.trim(), ip: ip.trim(), enabled: true, target })
    scheduleSave()
    return id
  }

  function updateSite(id: string, patch: Partial<Omit<Site, 'id'>>): void {
    const s = config.value.sites.find((x) => x.id === id)
    if (!s) return
    if (patch.domain !== undefined) s.domain = patch.domain.trim()
    if (patch.ip !== undefined) s.ip = patch.ip.trim()
    if (patch.enabled !== undefined) s.enabled = patch.enabled
    if (patch.target !== undefined) s.target = patch.target
    scheduleSave()
  }

  function deleteSite(id: string): void {
    const idx = config.value.sites.findIndex((x) => x.id === id)
    if (idx < 0) return
    config.value.sites.splice(idx, 1)
    scheduleSave()
  }

  function toggleSite(id: string): void {
    const s = config.value.sites.find((x) => x.id === id)
    if (!s) return
    s.enabled = !s.enabled
    scheduleSave()
  }

  // Status sinkron + diff + daftar backup. Non-throwing (swallow → null).
  async function status(): Promise<SitesStatus | null> {
    try {
      return await invoke<SitesStatus>('sites_status')
    } catch (err) {
      console.error('[sites] status gagal:', err)
      return null
    }
  }

  // Terapkan sites ke file hosts (memicu UAC). THROWS bila gagal/dibatalkan —
  // caller (UI) menampilkan pesan error (mis. UAC ditolak). Return diff terpakai.
  async function apply(): Promise<DiffResult> {
    return await invoke<DiffResult>('sites_apply')
  }

  // Restore hosts dari backup terpilih (memicu UAC). THROWS bila gagal.
  async function restore(backupId: string): Promise<void> {
    await invoke<void>('sites_restore', { backupId })
  }

  return {
    sites,
    addSite,
    updateSite,
    deleteSite,
    toggleSite,
    status,
    apply,
    restore,
  }
}
