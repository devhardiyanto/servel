<script setup lang="ts">
import { ref } from 'vue'

export interface PrereqAction {
  label: string
  href?: string
  onClick?: () => void
  primary?: boolean
  icon?: string
}

const props = withDefaults(
  defineProps<{
    name: string
    installed: boolean
    running?: boolean
    installUrl?: string
    icon?: string
    okDesc?: string
    notFoundDesc?: string
    actions?: PrereqAction[]
    code?: string
  }>(),
  { running: true },
)

const copied = ref(false)

async function copyCode(): Promise<void> {
  if (!props.code) return
  await navigator.clipboard.writeText(props.code)
  copied.value = true
  setTimeout(() => { copied.value = false }, 1500)
}

function handleAction(action: PrereqAction): void {
  if (action.href) {
    window.open(action.href, '_blank', 'noopener')
  } else if (action.onClick) {
    action.onClick()
  }
}

const isOk = () => props.installed && (props.running === undefined || props.running)
</script>

<template>
  <div class="prereq-card" :class="{ 'prereq-card--ok': isOk() }">
    <div class="prereq-card__main">
      <div class="prereq-card__icon" :class="{ 'prereq-card__icon--ok': isOk() }">
        <span v-if="icon" class="prereq-card__icon-emoji">{{ icon }}</span>
        <span v-else class="prereq-card__icon-default">&#9670;</span>
      </div>
      <div class="prereq-card__body">
        <div class="prereq-card__title-row">
          <span class="prereq-card__name">{{ name }}</span>
          <span v-if="!installed" class="prereq-badge prereq-badge--missing">NOT FOUND</span>
          <span v-else-if="running === false" class="prereq-badge prereq-badge--stopped">STOPPED</span>
          <span v-else class="prereq-badge prereq-badge--ok">&#10003; INSTALLED</span>
        </div>
        <div class="prereq-card__desc">
          <template v-if="isOk()">
            {{ okDesc ?? '' }}
          </template>
          <template v-else>
            {{ notFoundDesc ?? '' }}<slot />
          </template>
        </div>
      </div>
    </div>

    <template v-if="!installed">
      <div v-if="actions && actions.length > 0" class="prereq-card__actions">
        <button
          v-for="(action, i) in actions"
          :key="i"
          class="prereq-card__action-btn"
          :class="action.primary ? 'prereq-card__action-btn--primary' : 'prereq-card__action-btn--outline'"
          @click="handleAction(action)"
        >
          <span v-if="action.icon">{{ action.icon }}</span>
          {{ action.label }}
        </button>
      </div>

      <div v-if="code" class="prereq-card__code-block">
        <span class="prereq-card__code-prefix">&#62;</span>
        <code class="prereq-card__code-text">{{ code }}</code>
        <button class="prereq-card__copy-btn" @click="copyCode">
          {{ copied ? '✓ copied' : 'copy' }}
        </button>
      </div>
    </template>
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
  flex-shrink: 0;
  transition: background 0.15s;
}

.prereq-card__icon--ok {
  background: color-mix(in srgb, var(--green) 15%, transparent);
}

.prereq-card__icon-emoji {
  font-size: 18px;
  line-height: 1;
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
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin-top: var(--space-3);
  padding-top: var(--space-3);
  border-top: 1px solid var(--border);
}

.prereq-card__action-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 500;
  padding: 6px 14px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s, opacity 0.15s;
  border: 1px solid;
}

.prereq-card__action-btn--primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--bg);
}

.prereq-card__action-btn--primary:hover {
  opacity: 0.88;
}

.prereq-card__action-btn--outline {
  background: transparent;
  border-color: var(--border);
  color: var(--text);
}

.prereq-card__action-btn--outline:hover {
  border-color: var(--surface2);
  background: color-mix(in srgb, var(--text) 5%, transparent);
}

.prereq-card__code-block {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-top: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: #0a0a0c;
  border-radius: 6px;
  border: 1px solid var(--border);
}

.prereq-card__code-prefix {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--dim);
  flex-shrink: 0;
}

.prereq-card__code-text {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
  flex: 1;
  min-width: 0;
}

.prereq-card__copy-btn {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 3px;
  flex-shrink: 0;
  transition: color 0.1s;
}

.prereq-card__copy-btn:hover {
  color: var(--accent);
}
</style>
