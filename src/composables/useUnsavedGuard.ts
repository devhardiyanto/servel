// Penjaga perubahan yang belum disimpan. Satu sumber saja yang bisa
// "kotor" pada satu waktu (Environment project yang sedang terbuka). Siapa pun
// yang akan membuang sumber itu (pindah project, tutup panel, pindah layar,
// ganti environment) memanggil `confirmLeave()` lebih dulu.
import { ref } from 'vue'

export interface UnsavedSource {
  /** Nama yang ditampilkan, mis. "Default" atau "staging". */
  label: string
  /** Pemilik perubahan, mis. nama project. */
  owner: string
  /** Simpan; `false` bila gagal (mis. validasi), sehingga tetap di tempat. */
  save: () => Promise<boolean>
  discard: () => void
}

const source = ref<UnsavedSource | null>(null)
const asking = ref<{ resolve: (leave: boolean) => void } | null>(null)
const saving = ref(false)

function setSource(s: UnsavedSource | null): void {
  source.value = s
  // Tidak ada lagi yang perlu disimpan: pertanyaan yang masih terbuka dianggap boleh lanjut.
  if (!s && asking.value && !saving.value) answer(true)
}

/** `true` = boleh lanjut (tidak ada perubahan, dibuang, atau tersimpan). */
function confirmLeave(): Promise<boolean> {
  if (!source.value) return Promise.resolve(true)
  if (asking.value) return Promise.resolve(false)
  return new Promise((resolve) => {
    asking.value = { resolve }
  })
}

function answer(leave: boolean): void {
  const a = asking.value
  asking.value = null
  a?.resolve(leave)
}

async function choose(choice: 'discard' | 'stay' | 'save'): Promise<void> {
  // Esc atau klik di luar dialog saat Save berjalan: tunggu hasil Save.
  if (saving.value) return
  const s = source.value
  if (choice === 'stay' || !s) return answer(choice !== 'stay')
  if (choice === 'discard') {
    s.discard()
    source.value = null
    return answer(true)
  }
  saving.value = true
  try {
    const ok = await s.save()
    if (ok) source.value = null
    answer(ok)
  } finally {
    saving.value = false
  }
}

export function useUnsavedGuard() {
  return { source, asking, saving, setSource, confirmLeave, choose }
}
