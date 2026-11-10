/** Engine container yang dipakai Servel. Mirror `RuntimeKind` di Rust. */
export type EngineKind = 'docker' | 'wslc'

/** Preferensi user di config (`containerRuntime`). */
export type EnginePreference = 'auto' | EngineKind

export interface PrereqStatus {
  /** Engine aktif hasil resolusi preferensi. */
  engine: EngineKind
  engine_installed: boolean
  /** Docker: daemon hidup. WSLC: sesi merespons (tidak ada daemon terpisah). */
  engine_running: boolean
  engine_version: string | null
  /** Windows: `wslc` tersedia (WSL >= 3.0.1). */
  wslc_available: boolean
  docker_available: boolean
  /** Windows: versi dari `wsl --version`. */
  wsl_version: string | null
  /** Windows: Servel berjalan elevated — sesi WSLC terpisah dari sesi biasa. */
  elevated: boolean
  phpvm_installed: boolean
  fnm_installed: boolean
  phpvm_version: string | null
  fnm_version: string | null
  /** v1.6: hanya deteksi. phpvm/fnm opsional; integrasi mise di v1.7. */
  mise_installed: boolean
  mise_version: string | null
}

export const ENGINE_LABEL: Record<EngineKind, string> = {
  docker: 'Docker',
  wslc: 'WSL containers',
}
