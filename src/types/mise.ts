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
  /** Label jenis hasil deteksi ("Laravel", "Vite · Vue", ...). Hanya tampilan. */
  kind?: string
  /** Perintah dev server; `{port}` diganti port project. Kosong = tidak dijalankan Kova. */
  command?: string
  /** Port tetap di localhost. 0 = belum ditetapkan. */
  port?: number
  /** Jalankan otomatis saat Kova dibuka. */
  autoStart?: boolean
  /** Environment tambahan untuk dev server, di atas environment mise. */
  env?: Record<string, string>
  /** Environment lain; hanya nilai yang berbeda dari Default. */
  envs?: ProjectEnv[]
  /** Environment yang dipakai saat start; kosong = Default. */
  activeEnv?: string | null
  /** Start project ikut menyalakan service di profilnya. */
  startServices?: boolean
  /** Kelompok di layar Projects; kosong atau id yang sudah dihapus = Ungrouped. */
  groupId?: string | null
  /** Windows, folder di distro WSL: jalankan command di dalam distro. */
  wsl?: boolean
  /** Run with: command (default), Dockerfile, atau compose. */
  runMode?: RunMode
  container?: ContainerSpec | null
}

export type RunMode = 'command' | 'dockerfile' | 'compose'

export interface ContainerSpec {
  /** Relatif terhadap folder project; kosong = Dockerfile. */
  dockerfile?: string
  /** Port app di dalam container; 0 = sama dengan port project. */
  containerPort?: number
  /** Docker: gabung ke network service Kova. */
  joinServices?: boolean
  /** Relatif terhadap folder project; kosong = compose.yaml. */
  composeFile?: string
  /** Service compose yang dilewati karena Kova sudah menjalankannya. */
  skipComposeServices?: string[]
}

export interface ProjectGroup {
  id: string
  name: string
  collapsed?: boolean
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
