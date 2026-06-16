<script setup lang="ts">
import { ref, computed, provide, onMounted } from 'vue'
import type { Component } from 'vue'
import { SetViewKey } from './types/navigation'
import type { ViewName } from './types/navigation'
import Onboarding from './views/Onboarding.vue'
import Dashboard from './views/Dashboard.vue'
import Settings from './views/Settings.vue'
import Logs from './views/Logs.vue'
import { useAutoStart } from './composables/useAutoStart'
import { useDockerStatus } from './composables/useDockerStatus'

const currentView = ref<ViewName>('onboarding')

const viewMap: Record<ViewName, Component> = {
  onboarding: Onboarding,
  dashboard: Dashboard,
  settings: Settings,
  logs: Logs,
}

const viewComponent = computed(() => viewMap[currentView.value])

function setView(view: ViewName): void {
  currentView.value = view
}

provide(SetViewKey, setView)

const { runAutoStartOnce } = useAutoStart()
const { refresh: refreshDockerStatus } = useDockerStatus()
onMounted(() => {
  void runAutoStartOnce()
  void refreshDockerStatus()
})
</script>

<template>
  <component :is="viewComponent" />
</template>
