export interface PrereqStatus {
  docker_installed: boolean
  docker_running: boolean
  phpvm_installed: boolean
  fnm_installed: boolean
  phpvm_version: string | null
  fnm_version: string | null
}
