// Buka URL di browser bawaan sistem.
//
// `window.open` di webview Tauri tidak membuka browser sistem: di WebView2 ia
// bisa membuka jendela webview kosong atau tidak melakukan apa-apa. Semua
// tautan keluar memakai plugin opener; `window.open` hanya cadangan saat
// berjalan di browser biasa (pengembangan tanpa Tauri).
import { openUrl } from '@tauri-apps/plugin-opener'

export async function openExternal(url: string): Promise<void> {
  try {
    await openUrl(url)
  } catch {
    window.open(url, '_blank', 'noopener')
  }
}
