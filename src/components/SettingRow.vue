<script setup lang="ts">
defineProps<{
  label: string
  desc?: string
}>()
</script>

<template>
  <div class="setting-row">
    <div class="setting-row__info">
      <span class="setting-row__label"><slot name="label">{{ label }}</slot></span>
      <span v-if="desc" class="setting-row__desc">{{ desc }}</span>
    </div>
    <div class="setting-row__action">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-6);
  padding: var(--space-4) 0;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
}

.setting-row:last-child {
  border-bottom: none;
}

.setting-row__info {
  display: flex;
  flex-direction: column;
  gap: 3px;
  flex: 1;
  min-width: 0;
}

.setting-row__label {
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
}

.setting-row__desc {
  font-family: var(--font-sans);
  font-size: 11px;
  color: var(--muted);
  line-height: 1.4;
}

.setting-row__action {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
}
</style>
