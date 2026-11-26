<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { ref, computed, provide, onMounted, onUnmounted, h, watch } from 'vue'
import { useStatusAnnouncer } from '@/composables/useStatusAnnouncer'
import { listen } from '@tauri-apps/api/event'
import type { Component } from 'vue'
import { SetViewKey } from './types/navigation'
import type { ViewName } from './types/navigation'
import Onboarding from './views/Onboarding.vue'
import Dashboard from './views/Dashboard.vue'
import Settings from './views/Settings.vue'
import LogsViewer from './views/LogsViewer.vue'
import ProjectsView from './views/ProjectsView.vue'
import NavRail from './components/shell/NavRail.vue'
import StatusBar from './components/shell/StatusBar.vue'
import CommandPalette from './components/CommandPalette.vue'
import ToastHost from './components/ui/ToastHost.vue'
import UpdateBanner from './components/shell/UpdateBanner.vue'
import AutoStartNotice from './components/shell/AutoStartNotice.vue'
import { useAutoStart } from './composables/useAutoStart'
import { useEngineStatus } from './composables/useEngineStatus'
import { useUpdateCheck } from './composables/useUpdateCheck'
import { useToast } from './composables/useToast'
import { openExternal } from './utils/openExternal'
import { useServices } from './composables/useServices'
import { useConfig } from './composables/useConfig'
import { useVersionContext } from './composables/useVersionContext'
import UnsavedChangesModal from './components/UnsavedChangesModal.vue'
import { useUnsavedGuard } from './composables/useUnsavedGuard'

// Onboarding hanya sampai pernah diselesaikan sekali. Sampai config
// terbaca, tidak ada layar yang dirender, supaya onboarding tidak berkedip.
const currentView = ref<ViewName | null>(null)
const quitting = ref<{ stopServices: boolean } | null>(null)
const palette = ref<InstanceType<typeof CommandPalette> | null>(null)

// Settings dipakai dua kali: halaman Settings dan halaman Runtimes.
const RuntimesView: Component = { render: () => h(Settings, { mode: 'runtimes' }) }
const SettingsView: Component = { render: () => h(Settings, { mode: 'settings' }) }

const viewMap: Record<Exclude<ViewName, 'onboarding'>, Component> = {
  projects: ProjectsView,
  services: Dashboard,
  runtimes: RuntimesView,
  logs: LogsViewer,
  settings: SettingsView,
}

const viewComponent = computed(() =>
  currentView.value === null || currentView.value === 'onboarding' ? null : viewMap[currentView.value],
)

async function setView(view: ViewName | 'dashboard'): Promise<void> {
  const next = view === 'dashboard' ? 'services' : view
  if (next === currentView.value) return
  // Isian Environment yang belum disimpan hilang saat layar diganti.
  if (!(await unsavedGuard.confirmLeave())) return
  currentView.value = next
}

const unsavedGuard = useUnsavedGuard()
provide(SetViewKey, setView)

const { runAutoStartOnce } = useAutoStart()
const { refresh: refreshEngineStatus } = useEngineStatus()
const { check: checkUpdate, startAutoCheck, consumeJustUpdated } = useUpdateCheck()
const updatedToast = useToast()
async function announceUpdated(): Promise<void> {
  const u = await consumeJustUpdated()
  if (!u) return
  updatedToast.show(`Kova updated to ${u.to}`, { action: { label: "What's new", run: () => void openExternal(u.url) } })
}

// Polling RAM cepat saat Services terlihat, jeda saat jendela tersembunyi.
const docHidden = ref(document.hidden)
document.addEventListener('visibilitychange', () => (docHidden.value = document.hidden))
watch(
  () => (docHidden.value ? 'paused' : currentView.value === 'services' ? 'fast' : 'normal'),
  (mode) => {
    void invoke('stats_set_mode', { mode }).catch(() => {})
  },
  { immediate: true },
)

// Settings → General → Interface size (zoom webview).
const uiCfg = useConfig()
watch(
  () => uiCfg.config.value.uiScale ?? 1,
  (scale) => {
    void getCurrentWebview().setZoom(scale).catch(() => {})
  },
  { immediate: true },
)
const { load: loadServices, setSelectedIds, reconcileSelectedWithRunning } = useServices()

// Pilihan service dipulihkan di sini juga, bukan hanya saat layar Services
// dibuka: palet perintah bisa menyalakan service dari layar mana pun.
async function restoreServiceSelection(): Promise<void> {
  await loadServices()
  const cfg = useConfig()
  if (!cfg.loaded.value) await cfg.load()
  const ids = cfg.config.value.selectedServiceIds
  if (ids.length > 0) setSelectedIds(ids)
  reconcileSelectedWithRunning()
}

async function chooseFirstView(): Promise<void> {
  const cfg = useConfig()
  try {
    if (!cfg.loaded.value) await cfg.load()
  } catch {
    // Config tidak terbaca: Projects menampilkan errornya dengan Try again.
    currentView.value = 'projects'
    return
  }
  currentView.value = cfg.config.value.onboarded ? 'projects' : 'onboarding'
}

const announcer = useStatusAnnouncer()
const versionCtx = useVersionContext()

// Kembali ke Kova (mis. setelah memasang WSL, Docker, atau mise di
// terminal) memeriksa ulang engine dan mise, jadi hasil deteksi lama tidak
// bertahan sampai Kova ditutup. Paling sering sekali per 10 detik.
let lastFocusCheck = 0
function onWindowFocus(): void {
  const now = Date.now()
  if (now - lastFocusCheck < 10_000) return
  lastFocusCheck = now
  void refreshEngineStatus()
  void versionCtx.refresh()
}

onMounted(() => {
  void chooseFirstView()
  void listen<{ stopServices: boolean }>('app-quitting', (e) => (quitting.value = e.payload))
  void runAutoStartOnce()
  void refreshEngineStatus()
  void checkUpdate()
  startAutoCheck()
  void announceUpdated()
  void restoreServiceSelection()
  lastFocusCheck = Date.now()
  window.addEventListener('focus', onWindowFocus)
})
onUnmounted(() => window.removeEventListener('focus', onWindowFocus))
</script>

<template>
  <div class="sr-only" role="status" aria-live="polite">{{ announcer.polite.value }}</div>
  <div class="sr-only" role="alert">{{ announcer.assertive.value }}</div>
  <div v-if="quitting" class="quitting" role="alert" aria-busy="true">
    <span class="quitting__spinner" aria-hidden="true"></span>
    <span class="quitting__title">{{ quitting.stopServices ? 'Stopping services…' : 'Quitting…' }}</span>
    <span class="quitting__hint">Kova closes when everything has stopped.</span>
  </div>
  <template v-else-if="currentView === null"></template>
  <Onboarding v-else-if="currentView === 'onboarding'" />
  <div v-else class="shell">
    <NavRail :current="currentView" @go="setView" @palette="palette?.show()" />
    <main class="shell-main">
      <UpdateBanner />
      <AutoStartNotice />
      <component :is="viewComponent" />
    </main>
    <StatusBar class="shell-status" />
    <CommandPalette ref="palette" :view="currentView" @go="setView" />
    <ToastHost />
    <UnsavedChangesModal />
  </div>
</template>

<style scoped>
.quitting {
  height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  background: var(--bg);
  color: var(--text);
}
.quitting__title { font-size: var(--text-md); font-weight: 600; }
.quitting__hint { font-size: var(--text-sm); color: var(--muted); }
.quitting__spinner {
  width: 20px;
  height: 20px;
  margin-bottom: var(--space-2);
  border: 2px solid var(--border-strong);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: quit-spin 0.8s linear infinite;
}
@keyframes quit-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .quitting__spinner { animation: none; } }
.shell {
  display: grid;
  grid-template-columns: 188px 1fr;
  grid-template-rows: 1fr auto;
  height: 100vh;
  overflow: hidden;
}
.shell-main {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
/* Logs memakai latar gelap sampai ke tepi: banner update di atasnya ikut latar
   header Logs, supaya tidak tampak sebagai pita terpisah. */
.shell-main:has(.view-logs-viewer) { background: var(--surface); }
.shell-main > :deep(*:not(.ub)) {
  flex: 1;
  min-height: 0;
}
.shell-status { grid-column: 1 / -1; }
</style>
