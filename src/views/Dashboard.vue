<script setup lang="ts">
import { inject } from 'vue'
import { SetViewKey } from '../types/navigation'

const setView = inject(SetViewKey)!
</script>

<template>
  <div class="view view-dashboard">
    <h1>Dashboard</h1>
    <p>Konten dashboard akan diisi di phase berikutnya.</p>
    <div class="nav-buttons">
      <button @click="setView('onboarding')">Onboarding</button>
      <button @click="setView('settings')">Settings</button>
      <button @click="setView('logs')">Logs</button>
    </div>
  </div>
</template>

<style scoped>
.view-dashboard {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100vh;
  gap: var(--space-4);
}

.nav-buttons {
  display: flex;
  gap: var(--space-2);
}

button {
  padding: var(--space-2) var(--space-4);
  background-color: var(--surface2);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: 6px;
  font-family: var(--font-sans);
  font-size: 14px;
  cursor: pointer;
}

button:hover {
  background-color: var(--dim);
}
</style>
