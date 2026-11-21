export interface PortMap {
  host: string
  container: string
}

export interface ServiceDef {
  id: string
  name: string
  category: 'core' | 'additional'
  image: string
  containerName: string
  ports: PortMap[]
  environment: Record<string, string>
  volumes?: string[]
  command?: string
  ramEstimateMb: number
}

export interface ServiceStatus {
  id: string
  containerName: string
  running: boolean
  state: string
  exitCode: number | null
}

export interface ServiceUiState {
  id: string
  selected: boolean
  status: 'not_created' | 'stopped' | 'running' | 'starting' | 'stopping' | 'error'
  memMb: number | null
  /** Pesan dari start/stop terakhir yang gagal (v3.8); dihapus saat dicoba lagi. */
  error?: string
}

export interface ContainerMemStat {
  id: string
  memMb: number
}

export interface PortConflict {
  serviceId: string
  serviceName: string
  port: number
}
