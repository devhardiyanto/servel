<script setup lang="ts">
import { ref, computed, provide } from 'vue'
import type { Component } from 'vue'
import { SetViewKey } from './types/navigation'
import type { ViewName } from './types/navigation'
import Onboarding from './views/Onboarding.vue'
import Dashboard from './views/Dashboard.vue'
import Settings from './views/Settings.vue'
import Logs from './views/Logs.vue'

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
</script>

<template>
  <component :is="viewComponent" />
</template>
