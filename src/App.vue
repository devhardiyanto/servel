<script setup lang="ts">
import { ref, computed, provide, onMounted, h } from 'vue'
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
import { useAutoStart } from './composables/useAutoStart'
import { useEngineStatus } from './composables/useEngineStatus'
import { useUpdateCheck } from './composables/useUpdateCheck'
import { useServices } from './composables/useServices'

const currentView = ref<ViewName>('onboarding')

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
  currentView.value === 'onboarding' ? null : viewMap[currentView.value],
)

function setView(view: ViewName | 'dashboard'): void {
  currentView.value = view === 'dashboard' ? 'services' : view
}

provide(SetViewKey, setView)

const { runAutoStartOnce } = useAutoStart()
const { refresh: refreshEngineStatus } = useEngineStatus()
const { check: checkUpdate } = useUpdateCheck()
const { load: loadServices } = useServices()
onMounted(() => {
  void runAutoStartOnce()
  void refreshEngineStatus()
  void checkUpdate()
  void loadServices()
})
</script>

<template>
  <Onboarding v-if="currentView === 'onboarding'" />
  <div v-else class="shell">
    <NavRail :current="currentView" @go="setView" />
    <main class="shell-main">
      <component :is="viewComponent" />
    </main>
    <StatusBar class="shell-status" />
  </div>
</template>

<style scoped>
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
.shell-main > :deep(*) {
  flex: 1;
  min-height: 0;
}
.shell-status { grid-column: 1 / -1; }
</style>
