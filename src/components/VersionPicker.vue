<script setup lang="ts">
interface VersionItem {
  version: string
  active: boolean
}

const props = withDefaults(defineProps<{
  label: string
  accent: string
  versions: VersionItem[]
  active: string | null
  switching?: boolean
  disabled?: boolean
}>(), {
  switching: false,
  disabled: false,
})

const emit = defineEmits<{
  (e: 'switch', version: string): void
  (e: 'install', version: string): void
}>()

function handleChipClick(version: string): void {
  if (props.disabled || props.switching) return
  if (version !== props.active) emit('switch', version)
}

function stepVersion(dir: number): void {
  if (props.disabled || props.switching || props.versions.length === 0) return
  const idx = props.versions.findIndex((v) => v.version === props.active)
  const current = idx === -1 ? 0 : idx
  const next = Math.min(props.versions.length - 1, Math.max(0, current + dir))
  if (props.versions[next].version !== props.active) {
    emit('switch', props.versions[next].version)
  }
}

function currentIndex(): number {
  return props.versions.findIndex((v) => v.version === props.active)
}
</script>

<template>
  <div
    class="version-picker"
    :class="{ 'version-picker--disabled': disabled }"
    :style="{ borderColor: accent + '40' }"
  >
    <div class="vp-label">
      {{ label }}
      <span v-if="active && !disabled" class="vp-live" :style="{ color: accent }">
        <span class="vp-live-dot" :style="{ background: accent }"></span>
        active
      </span>
      <span v-else-if="disabled" class="vp-disabled-badge">unavailable</span>
    </div>

    <div class="vp-ver-row">
      <span class="vp-ver-tag" :style="{ color: accent }">
        {{ active ?? '—' }}
      </span>
      <span v-if="switching" class="vp-switching">switching...</span>
      <span class="vp-spacer"></span>
      <button
        class="vp-arrow"
        :disabled="disabled || switching || currentIndex() <= 0"
        @click="stepVersion(-1)"
      >&#8249;</button>
      <button
        class="vp-arrow"
        :disabled="disabled || switching || currentIndex() >= versions.length - 1"
        @click="stepVersion(1)"
      >&#8250;</button>
    </div>

    <div class="vp-chip-list">
      <button
        v-for="item in versions"
        :key="item.version"
        class="vp-chip"
        :class="{ 'vp-chip--active': item.version === active }"
        :style="item.version === active
          ? { background: accent + '14', color: accent, borderColor: accent + '40' }
          : {}"
        :disabled="disabled || switching"
        @click="handleChipClick(item.version)"
      >
        {{ item.version }}
      </button>
      <span v-if="versions.length === 0 && !disabled" class="vp-empty">
        tidak ada versi terinstal
      </span>
      <span v-if="disabled" class="vp-empty">
        Akan diisi Task 4
      </span>
    </div>
  </div>
</template>

<style scoped>
.version-picker {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: var(--space-4);
  transition: border-color 0.15s, opacity 0.15s;
}

.version-picker--disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.vp-label {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-family: var(--font-sans);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
  margin-bottom: var(--space-3);
}

.vp-live {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  font-weight: 400;
  letter-spacing: 0.04em;
}

.vp-live-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

.vp-disabled-badge {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--dim);
  padding: 1px 5px;
  border: 1px solid var(--border);
  border-radius: 3px;
}

.vp-ver-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-3);
}

.vp-ver-tag {
  font-family: var(--font-mono);
  font-size: 20px;
  font-weight: 400;
  line-height: 1;
}

.vp-switching {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
}

.vp-spacer {
  flex: 1;
}

.vp-arrow {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text);
  font-size: 16px;
  cursor: pointer;
  padding: 0;
  line-height: 1;
  transition: background 0.1s;
}

.vp-arrow:hover:not(:disabled) {
  background: var(--dim);
}

.vp-arrow:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.vp-chip-list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.vp-chip {
  font-family: var(--font-mono);
  font-size: 12px;
  padding: 3px 10px;
  border-radius: 4px;
  border: 1px solid var(--border);
  background: var(--surface2);
  color: var(--muted);
  cursor: pointer;
  transition: background 0.1s, color 0.1s, border-color 0.1s;
}

.vp-chip:hover:not(:disabled):not(.vp-chip--active) {
  background: var(--dim);
  color: var(--text);
}

.vp-chip--active {
  font-weight: 600;
}

.vp-chip:disabled {
  cursor: not-allowed;
}

.vp-empty {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
}
</style>
