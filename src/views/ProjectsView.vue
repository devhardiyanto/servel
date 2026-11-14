<script setup lang="ts">
// Projects (v3): layar pertama setelah onboarding.
import ProjectsPanel from '@/components/ProjectsPanel.vue'
</script>

<template>
  <div class="page">
    <ProjectsPanel />
  </div>
</template>

<style scoped>
.page {
  height: 100%;
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
}
</style>
