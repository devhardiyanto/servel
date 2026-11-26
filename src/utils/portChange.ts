import { ref } from 'vue'

/** Perubahan port service terakhir; Connect menampilkannya sampai ditutup. */
export interface PortChange {
  serviceId: string
  from: number
  to: number
  /** Project yang Environment-nya masih memuat port lama. */
  projects: string[]
}

export const lastPortChange = ref<PortChange | null>(null)
