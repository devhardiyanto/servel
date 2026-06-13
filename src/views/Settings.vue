<script setup lang="ts">
import { ref, inject, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ask } from '@tauri-apps/plugin-dialog'
import { SetViewKey } from '@/types/navigation'
import { useConfig } from '@/composables/useConfig'
import { useLogs } from '@/composables/useLogs'
import SettingRow from '@/components/SettingRow.vue'
import type { PrereqStatus } from '@/types/prereq'

type SettingsNav = 'general' | 'services' | 'php_node' | 'about'

const setView = inject(SetViewKey)!

const {
  config,
  loaded,
  load,
  setAutoStart,
  setRememberSession,
  setMinimizeToTray,
  reset,
} = useConfig()

const activeNav = ref<SettingsNav>('general')

const composePath = ref<string>('')
const dockerRunning = ref<boolean | null>(null)
const checkingDocker = ref(false)

async function fetchComposePath(): Promise<void> {
  try {
    composePath.value = await invoke<string>('get_compose_path')
  } catch {
    composePath.value = '—'
  }
}

async function checkDocker(): Promise<void> {
  checkingDocker.value = true
  try {
    const result = await invoke<PrereqStatus>('check_prerequisites')
    dockerRunning.value = result.docker_running
  } catch {
    dockerRunning.value = false
  } finally {
    checkingDocker.value = false
  }
}

async function handleReset(): Promise<void> {
  let ok = false
  try {
    ok = await ask(
      'Reset all settings? This will restore defaults and clear saved selection.',
      { title: 'Reset Settings', kind: 'warning' },
    )
  } catch {
    ok = window.confirm('Reset all settings?')
  }
  if (!ok) return
  await reset()
  const { push } = useLogs('SERVEL')
  push({ ts: new Date().toTimeString().slice(0, 8), src: 'SERVEL', text: 'settings reset to defaults' })
}

onMounted(async () => {
  if (!loaded.value) await load()
  await fetchComposePath()
  await checkDocker()
})
</script>

<template>
  <div class="view view-settings">
    <header class="app-strip">
      <button class="app-strip__back" @click="setView('dashboard')">&#8592;</button>
      <span class="app-strip__title">SETTINGS</span>
      <span class="app-strip__spacer"></span>
    </header>

    <div class="settings-body">
      <nav class="settings-nav">
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'general' }"
          @click="activeNav = 'general'"
        >General</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'services' }"
          @click="activeNav = 'services'"
        >Services</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'php_node' }"
          @click="activeNav = 'php_node'"
        >PHP &amp; Node</button>
        <button
          class="snav-item"
          :class="{ 'snav-item--active': activeNav === 'about' }"
          @click="activeNav = 'about'"
        >About</button>
      </nav>

      <main class="settings-content">
        <template v-if="activeNav === 'general'">
          <div class="section-block">
            <div class="section-title">GENERAL</div>

            <SettingRow
              label="Auto-start infra on launch"
              desc="Start saved selection automatically when Servel boots"
            >
              <button
                class="toggle"
                :class="{ 'toggle--on': config.autoStart }"
                role="switch"
                :aria-checked="config.autoStart"
                @click="setAutoStart(!config.autoStart)"
              >
                <span class="toggle-knob"></span>
              </button>
            </SettingRow>

            <SettingRow
              label="Remember last session"
              desc="Persist selected services between launches"
            >
              <button
                class="toggle"
                :class="{ 'toggle--on': config.rememberSession }"
                role="switch"
                :aria-checked="config.rememberSession"
                @click="setRememberSession(!config.rememberSession)"
              >
                <span class="toggle-knob"></span>
              </button>
            </SettingRow>

            <SettingRow
              label="Minimize to tray on close"
              desc="Keep Servel running in system tray instead of quitting on window close"
            >
              <button
                class="toggle"
                :class="{ 'toggle--on': config.minimizeToTray }"
                role="switch"
                :aria-checked="config.minimizeToTray"
                @click="setMinimizeToTray(!config.minimizeToTray)"
              >
                <span class="toggle-knob"></span>
              </button>
            </SettingRow>
          </div>

          <div class="section-block">
            <div class="section-title">DOCKER</div>

            <SettingRow
              label="Docker Compose file path"
              desc="Path to generated compose file"
            >
              <span class="setting-value-mono">{{ composePath || '—' }}</span>
            </SettingRow>

            <SettingRow label="Docker status">
              <span
                class="status-pill"
                :class="dockerRunning === true ? 'status-pill--green' : 'status-pill--red'"
              >
                <span class="status-dot"></span>
                <span v-if="dockerRunning === true">Running</span>
                <span v-else-if="dockerRunning === false">Stopped</span>
                <span v-else>—</span>
              </span>
              <button
                class="action-btn"
                :disabled="checkingDocker"
                @click="checkDocker"
              >{{ checkingDocker ? '...' : 'Refresh' }}</button>
            </SettingRow>
          </div>

          <div class="section-block section-block--danger">
            <div class="section-title section-title--red">DANGER ZONE</div>

            <SettingRow
              label="Reset all settings"
              desc="Restore defaults and clear saved selection"
            >
              <button class="action-btn action-btn--red" @click="handleReset">Reset</button>
            </SettingRow>
          </div>
        </template>

        <template v-else-if="activeNav === 'services'">
          <div class="section-block">
            <div class="section-title">SERVICES</div>
            <p class="placeholder-text">Service configuration — coming soon.</p>
          </div>
        </template>

        <template v-else-if="activeNav === 'php_node'">
          <div class="section-block">
            <div class="section-title">PHP &amp; NODE</div>
            <p class="placeholder-text">PHP &amp; Node configuration — coming soon.</p>
          </div>
        </template>

        <template v-else-if="activeNav === 'about'">
          <div class="section-block">
            <div class="section-title">ABOUT</div>
            <div class="about-block">
              <span class="about-name">servel</span>
              <span class="about-desc">Local dev environment manager</span>
              <span class="about-version">v1.0.0</span>
            </div>
          </div>
        </template>
      </main>
    </div>
  </div>
</template>

<style scoped>
.view-settings {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
}

.app-strip {
  display: flex;
  align-items: center;
  padding: 0 var(--space-4);
  height: 36px;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  background: var(--surface);
  flex-shrink: 0;
  gap: var(--space-4);
}

.app-strip__back {
  background: transparent;
  border: none;
  color: var(--muted);
  font-size: 16px;
  cursor: pointer;
  padding: 0 var(--space-2);
  line-height: 1;
  border-radius: 4px;
  transition: color 0.1s, background 0.1s;
}

.app-strip__back:hover {
  color: var(--text);
  background: var(--surface2);
}

.app-strip__title {
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: var(--text);
  flex: 1;
  text-align: center;
}

.app-strip__spacer {
  width: 32px;
}

.settings-body {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.settings-nav {
  width: 180px;
  flex-shrink: 0;
  border-right: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  background: var(--surface);
  padding: var(--space-4) 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.snav-item {
  display: block;
  width: 100%;
  text-align: left;
  background: transparent;
  border: none;
  border-left: 2px solid transparent;
  padding: var(--space-2) var(--space-4);
  font-family: var(--font-mono);
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--muted);
  cursor: pointer;
  transition: color 0.1s, border-color 0.1s, background 0.1s;
}

.snav-item:hover:not(.snav-item--active) {
  color: var(--text);
  background: color-mix(in srgb, var(--surface2) 50%, transparent);
}

.snav-item--active {
  color: var(--accent);
  border-left-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}

.settings-content {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.section-block {
  display: flex;
  flex-direction: column;
}

.section-block--danger {
  margin-top: var(--space-2);
}

.section-title {
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--dim);
  margin-bottom: var(--space-3);
}

.section-title--red {
  color: color-mix(in srgb, var(--red) 80%, transparent);
}

/* Toggle */
.toggle {
  position: relative;
  width: 36px;
  height: 20px;
  border-radius: 10px;
  background: var(--surface2);
  border: 1px solid var(--border);
  cursor: pointer;
  padding: 0;
  transition: background 0.15s, border-color 0.15s;
}

.toggle--on {
  background: color-mix(in srgb, var(--accent) 35%, transparent);
  border-color: color-mix(in srgb, var(--accent) 60%, transparent);
}

.toggle-knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--muted);
  transition: transform 0.15s, background 0.15s;
}

.toggle--on .toggle-knob {
  transform: translateX(16px);
  background: var(--accent);
}

/* Status pill */
.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 10px;
  border-radius: 20px;
  font-family: var(--font-mono);
  font-size: 11px;
  border: 1px solid;
}

.status-pill--green {
  background: color-mix(in srgb, var(--green) 12%, transparent);
  border-color: color-mix(in srgb, var(--green) 35%, transparent);
  color: var(--green);
}

.status-pill--red {
  background: color-mix(in srgb, var(--red) 12%, transparent);
  border-color: color-mix(in srgb, var(--red) 35%, transparent);
  color: var(--red);
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

/* Mono value display */
.setting-value-mono {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--muted);
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Action buttons */
.action-btn {
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 4px 12px;
  border-radius: 4px;
  cursor: pointer;
  background: var(--surface2);
  border: 1px solid var(--border);
  color: var(--muted);
  transition: background 0.1s, color 0.1s;
}

.action-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--surface2) 80%, var(--text) 20%);
  color: var(--text);
}

.action-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.action-btn--red {
  background: color-mix(in srgb, var(--red) 10%, transparent);
  border-color: color-mix(in srgb, var(--red) 40%, transparent);
  color: var(--red);
}

.action-btn--red:hover:not(:disabled) {
  background: color-mix(in srgb, var(--red) 20%, transparent);
  border-color: color-mix(in srgb, var(--red) 65%, transparent);
}

/* About section */
.about-block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-4);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
}

.about-name {
  font-family: var(--font-mono);
  font-size: 18px;
  font-weight: 700;
  color: var(--accent);
}

.about-desc {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--muted);
}

.about-version {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
}

/* Placeholder */
.placeholder-text {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--dim);
  margin: 0;
  padding: var(--space-4) 0;
}
</style>
