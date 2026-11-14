<script setup lang="ts">
import { ref, computed, provide, onMounted } from 'vue'
import type { Component } from 'vue'
import { SetViewKey } from './types/navigation'
import type { ViewName } from './types/navigation'
import Onboarding from './views/Onboarding.vue'
import Dashboard from './views/Dashboard.vue'
import Settings from './views/Settings.vue'
import LogsViewer from './views/LogsViewer.vue'
import { useAutoStart } from './composables/useAutoStart'
import { useEngineStatus } from './composables/useEngineStatus'
import { useUpdateCheck } from './composables/useUpdateCheck'

const currentView = ref<ViewName>('onboarding')

const viewMap: Record<ViewName, Component> = {
  onboarding: Onboarding,
  dashboard: Dashboard,
  settings: Settings,
  logs: LogsViewer,
}

const viewComponent = computed(() => viewMap[currentView.value])

function setView(view: ViewName): void {
  currentView.value = view
}

provide(SetViewKey, setView)

const { runAutoStartOnce } = useAutoStart()
const { refresh: refreshEngineStatus } = useEngineStatus()
const { check: checkUpdate } = useUpdateCheck()
onMounted(() => {
  void runAutoStartOnce()
  void refreshEngineStatus()
  void checkUpdate()
})
</script>

<template>
  <component :is="viewComponent" />
</template>
