// Lokasi yang dipilih di Runtimes: '' = PC ini, selain itu nama distro WSL.
// Dibagi antara daftar versi (RuntimeTools) dan seksi Version manager, supaya
// keduanya menunjuk mise yang sama.
import { ref } from 'vue'

const loc = ref('')
/** Versi mise di distro terpilih; undefined = belum dicek, null = tidak ada. */
const distroMise = ref<string | null | undefined>(undefined)

export function useRuntimeLocation() {
  return { loc, distroMise }
}
