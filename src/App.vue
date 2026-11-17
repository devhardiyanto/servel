<script setup lang="ts">
import { ref, computed, provide, onMounted, h } from 'vue'
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
import { useAutoStart } from './composables/useAutoStart'
import { useEngineStatus } from './composables/useEngineStatus'
import { useUpdateCheck } from './composables/useUpdateCheck'
import { useServices } from './composables/useServices'
import { useConfig } from './composables/useConfig'

// v3.5.1: onboarding hanya sampai pernah diselesaikan sekali. Sampai config
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

function setView(view: ViewName | 'dashboard'): void {
  currentView.value = view === 'dashboard' ? 'services' : view
}

provide(SetViewKey, setView)

const { runAutoStartOnce } = useAutoStart()
const { refresh: refreshEngineStatus } = useEngineStatus()
const { check: checkUpdate } = useUpdateCheck()
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

onMounted(() => {
  void chooseFirstView()
  void listen<{ stopServices: boolean }>('app-quitting', (e) => (quitting.value = e.payload))
  void runAutoStartOnce()
  void refreshEngineStatus()
  void checkUpdate()
  void restoreServiceSelection()
})
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
      <component :is="viewComponent" />
    </main>
    <StatusBar class="shell-status" />
    <CommandPalette ref="palette" @go="setView" />
    <ToastHost />
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
.shell-main > :deep(*:not(.ub)) {
  flex: 1;
  min-height: 0;
}
.shell-status { grid-column: 1 / -1; }
</style>
