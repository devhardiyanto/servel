<script setup lang="ts">
defineProps<{
  runningCount: number
  totalRamMb: number
  uptime: string
}>()
</script>

<template>
  <div class="status-bar">
    <span class="status-bar__item">
      <span class="status-bar__dot status-bar__dot--green"></span>
      {{ runningCount }} running
    </span>
    <span class="status-bar__sep">·</span>
    <span class="status-bar__item status-bar__item--amber">
      {{ totalRamMb }} MB RAM
    </span>
    <span class="status-bar__sep">·</span>
    <span class="status-bar__item">
      uptime {{ uptime }}
    </span>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  background: var(--surface);
  border-top: 1px solid var(--border);
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
}

.status-bar__item {
  display: flex;
  align-items: center;
  gap: 5px;
}

.status-bar__item--amber {
  color: var(--amber);
}

.status-bar__sep {
  color: var(--dim);
}

.status-bar__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-bar__dot--green {
  background: var(--green);
}
</style>
