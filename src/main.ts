import '@fontsource/inter/400.css'
import '@fontsource/inter/500.css'
import '@fontsource/inter/600.css'
import '@fontsource/space-mono/400.css'

import './assets/styles/tokens.css'
import './assets/styles/base.css'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { invoke } from '@tauri-apps/api/core'

const app = createApp(App)
app.use(createPinia())
app.mount('#app')

// Jendela mulai tersembunyi (lihat `app_ready` di src-tauri/src/lib.rs) dan
// ditampilkan setelah UI ter-mount. Tidak memakai requestAnimationFrame:
// webview yang tersembunyi bisa menunda callback itu.
void invoke('app_ready').catch(() => {})
