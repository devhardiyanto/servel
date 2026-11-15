// Aplikasi berjalan di webview (WebView2 / WKWebView / WebKitGTK) yang masih
// membawa pintasan browser. Beberapa di antaranya merusak aplikasi: muat ulang
// membuang state, cetak dan "cari di halaman" membuka UI browser, zoom
// menggeser tata letak, seret file membuat webview membuka file itu.
//
// Yang dimatikan hanya perilaku bawaan (preventDefault). Event tetap sampai ke
// handler aplikasi, jadi Ctrl K (palet), Ctrl F di Logs, dan Ctrl A di area log
// tetap bekerja. Salin, tempel, potong, undo di kolom teks tidak disentuh.

/** Pintasan dengan Ctrl (atau Cmd) saja. */
const CTRL_KEYS = new Set([
  'p', // cetak
  'r', // muat ulang
  'f', // cari di halaman
  'g', // cari berikutnya
  'u', // lihat sumber
  's', // simpan halaman
  'o', // buka file
  'h', // riwayat
  'j', // unduhan
  'd', // markah
  'n', // jendela baru
  't', // tab baru
  'w', // tutup tab
  'e', // cari di web
  'l', // fokus bilah alamat
  '+', '=', '-', '_', '0', // zoom
])

/** Pintasan dengan Ctrl + Shift. */
const CTRL_SHIFT_KEYS = new Set([
  'i', 'j', 'c', // devtools
  'r', // muat ulang paksa
  'p', // cetak (dialog sistem)
  'n', // jendela InPrivate
  't', // buka lagi tab
  's', // tangkapan layar web
  'delete', // hapus data penjelajahan
  '+', '=', '-', '_',
])

/** Tombol tanpa pengubah. */
const BARE_KEYS = new Set(['f3', 'f5', 'f6', 'f7', 'f12', 'browserback', 'browserforward', 'browserrefresh', 'browserfavorites', 'browsersearch', 'browserhome'])

export interface KeyLike {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  shiftKey: boolean
  altKey: boolean
}

function isEditable(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el || typeof el.tagName !== 'string') return false
  return el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable === true
}

/** `true` bila perilaku bawaan webview untuk tombol ini harus dimatikan. */
export function shouldBlockKey(e: KeyLike, editable: boolean): boolean {
  const key = e.key.toLowerCase()
  const mod = e.ctrlKey || e.metaKey
  if (BARE_KEYS.has(key)) return true
  // Shift F10 = menu konteks dari keyboard; Shift Esc = task manager browser.
  if (e.shiftKey && !mod && !e.altKey && (key === 'f10' || key === 'escape')) return true
  if (e.altKey && !mod) {
    // Alt ← / → = mundur / maju di riwayat; Alt Home = halaman awal.
    return key === 'arrowleft' || key === 'arrowright' || key === 'home'
  }
  if (!mod || e.altKey) return false
  if (e.shiftKey) return CTRL_SHIFT_KEYS.has(key)
  if (CTRL_KEYS.has(key)) return true
  // Ctrl A di luar kolom teks memilih seluruh jendela.
  if (key === 'a') return !editable
  return false
}

/** Pasang sekali saat aplikasi dimulai. Tidak dipasang di mode pengembangan. */
export function installWebviewGuards(): void {
  window.addEventListener(
    'keydown',
    (e) => {
      if (shouldBlockKey(e, isEditable(e.target))) e.preventDefault()
    },
    { capture: true },
  )
  // Menu konteks bawaan (Back, Reload, Save as, Print, Inspect). Di kolom teks
  // dibiarkan supaya tempel lewat klik kanan tetap ada.
  window.addEventListener('contextmenu', (e) => {
    if (!isEditable(e.target)) e.preventDefault()
  })
  // Ctrl + roda mouse = zoom.
  window.addEventListener(
    'wheel',
    (e) => {
      if (e.ctrlKey || e.metaKey) e.preventDefault()
    },
    { passive: false },
  )
  // File yang dijatuhkan ke jendela dibuka webview, menggantikan aplikasi.
  for (const type of ['dragover', 'drop'] as const) {
    window.addEventListener(type, (e) => e.preventDefault())
  }
  // Klik tengah pada tautan membuka jendela baru; tombol mundur/maju mouse.
  window.addEventListener('auxclick', (e) => e.preventDefault())
  window.addEventListener('mouseup', (e) => {
    if (e.button === 3 || e.button === 4) e.preventDefault()
  })
}
