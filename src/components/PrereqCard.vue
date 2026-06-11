<script setup lang="ts">
withDefaults(
  defineProps<{
    name: string
    installed: boolean
    running?: boolean
    installUrl: string
    icon?: string
  }>(),
  { running: true },
)
</script>

<template>
  <div class="prereq-card" :class="{ 'prereq-card--ok': installed && (running === undefined || running) }">
    <div class="prereq-card__main">
      <div class="prereq-card__icon" :class="{ 'prereq-card__icon--ok': installed && (running === undefined || running) }">
        <span v-if="icon">{{ icon }}</span>
        <span v-else class="prereq-card__icon-default">◆</span>
      </div>
      <div class="prereq-card__body">
        <div class="prereq-card__title-row">
          <span class="prereq-card__name">{{ name }}</span>
          <span v-if="!installed" class="prereq-badge prereq-badge--missing">NOT FOUND</span>
          <span v-else-if="running === false" class="prereq-badge prereq-badge--stopped">STOPPED</span>
          <span v-else class="prereq-badge prereq-badge--ok">INSTALLED</span>
        </div>
        <div class="prereq-card__desc">
          <slot />
        </div>
      </div>
    </div>

    <div v-if="!installed" class="prereq-card__actions">
      <a :href="installUrl" target="_blank" rel="noopener" class="prereq-card__link">
        View install guide ↗
      </a>
    </div>
  </div>
</template>

<style scoped>
.prereq-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: var(--space-4);
  transition: border-color 0.15s;
}

.prereq-card--ok {
  border-color: color-mix(in srgb, var(--green) 40%, transparent);
}

.prereq-card__main {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
}

.prereq-card__icon {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  background: var(--surface2);
  font-size: 18px;
  flex-shrink: 0;
  transition: background 0.15s;
}

.prereq-card__icon--ok {
  background: color-mix(in srgb, var(--green) 15%, transparent);
}

.prereq-card__icon-default {
  font-size: 14px;
  color: var(--dim);
}

.prereq-card__body {
  flex: 1;
  min-width: 0;
}

.prereq-card__title-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-1);
}

.prereq-card__name {
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.prereq-card__desc {
  font-family: var(--font-sans);
  font-size: 13px;
  color: var(--muted);
  line-height: 1.5;
}

.prereq-badge {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 400;
  letter-spacing: 0.05em;
  padding: 2px 6px;
  border-radius: 4px;
  flex-shrink: 0;
}

.prereq-badge--ok {
  background: color-mix(in srgb, var(--green) 15%, transparent);
  color: var(--green);
  border: 1px solid color-mix(in srgb, var(--green) 30%, transparent);
}

.prereq-badge--missing {
  background: color-mix(in srgb, var(--red) 15%, transparent);
  color: var(--red);
  border: 1px solid color-mix(in srgb, var(--red) 30%, transparent);
}

.prereq-badge--stopped {
  background: color-mix(in srgb, var(--amber) 15%, transparent);
  color: var(--amber);
  border: 1px solid color-mix(in srgb, var(--amber) 30%, transparent);
}

.prereq-card__actions {
  margin-top: var(--space-3);
  padding-top: var(--space-3);
  border-top: 1px solid var(--border);
}

.prereq-card__link {
  font-family: var(--font-sans);
  font-size: 13px;
  color: var(--accent);
  text-decoration: none;
  transition: opacity 0.15s;
}

.prereq-card__link:hover {
  opacity: 0.8;
}
</style>
