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

export interface Project {
  id: string
  name: string
  path: string
  /** Profil service yang diterapkan saat project dipilih (v1.9). */
  profileId?: string | null
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
