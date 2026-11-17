import '@fontsource/inter/400.css'
import '@fontsource/inter/500.css'
import '@fontsource/inter/600.css'
import '@fontsource/space-mono/400.css'

import './assets/styles/tokens.css'
import './assets/styles/base.css'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import TrayPopup from './views/TrayPopup.vue'
import { invoke } from '@tauri-apps/api/core'
import { installWebviewGuards } from './utils/webviewGuards'

// Pintasan browser bawaan webview dimatikan di build rilis saja, supaya
// muat ulang dan devtools tetap ada saat pengembangan.
if (import.meta.env.PROD) installWebviewGuards()

// Jendela popup tray (v3.7.0) memuat halaman yang sama dengan `?view=tray`.
const isTrayPopup = new URLSearchParams(location.search).get('view') === 'tray'
if (isTrayPopup) document.documentElement.classList.add('tray-popup')

const app = createApp(isTrayPopup ? TrayPopup : App)
app.use(createPinia())
app.mount('#app')

// Jendela utama mulai tersembunyi (lihat `app_ready` di src-tauri/src/lib.rs)
// dan ditampilkan setelah UI ter-mount. Tidak memakai requestAnimationFrame:
// webview yang tersembunyi bisa menunda callback itu. Popup tray menampilkan
// dirinya sendiri lewat `tray_popup_resize`.
if (!isTrayPopup) void invoke('app_ready').catch(() => {})
