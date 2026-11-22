/** Mirror `ToolCurrent` / `MiseContext` di src-tauri/src/mise.rs. */
export type SourceScope = 'global' | 'project' | 'none'

export interface ToolCurrent {
  tool: string
  version: string
  requested: string | null
  source: string | null
  scope: SourceScope
  installed: boolean
}

export interface ProjectEnv {
  name: string
  vars: Record<string, string>
}

export interface Project {
  id: string
  name: string
  path: string
  /** Profil service yang diterapkan saat project dipilih. */
  profileId?: string | null
  /** Jenis hasil deteksi (laravel, vite, ...). Hanya label. */
  kind?: string
  /** Perintah dev server; `{port}` diganti port project. Kosong = tidak dijalankan Kova. */
  command?: string
  /** Port tetap di localhost. 0 = belum ditetapkan. */
  port?: number
  /** Jalankan otomatis saat Kova dibuka. */
  autoStart?: boolean
  /** Environment Default untuk dev server, di atas environment mise. */
  env?: Record<string, string>
  /** Environment lain; hanya nilai yang berbeda dari Default. */
  envs?: ProjectEnv[]
  /** Environment yang dipakai saat start; kosong = Default. */
  activeEnv?: string | null
  /** Start project ikut menyalakan service di profilnya. */
  startServices?: boolean
}

export interface MiseContext {
  provider: 'mise' | 'legacy'
  miseVersion: string | null
  project: Project | null
  projectMissing: boolean
  php: ToolCurrent | null
  node: ToolCurrent | null
  error: string | null
}

/** Prefix error dari backend (lihat ERR_* di mise.rs). */
export const MISE_UNTRUSTED = 'MISE_UNTRUSTED'
export const MISE_NEEDS_SCOPE = 'MISE_NEEDS_SCOPE'

/** Tool di project masih mengikuti global — UI menanyakan scope. */
export interface ScopeRequest {
  tool: 'php' | 'node'
  version: string
  projectFile: string
  globalVersion: string | null
}
