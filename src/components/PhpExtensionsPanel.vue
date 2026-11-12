<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

/** Mirror `PhpExtStatus` di src-tauri/src/php_ext.rs. */
interface PhpExt {
  name: string
  loaded: boolean
  available: boolean
  builtin: boolean
}

interface HttpsStatus {
  ok: boolean
  cainfo: string | null
  cafile: string | null
}

interface PhpExtStatus {
  version: string
  binary: string
  iniPath: string
  iniExists: boolean
  extensionDir: string
  extensions: PhpExt[]
  https: HttpsStatus | null
}

interface PhpVersion {
  version: string
  active: boolean
}

const versions = ref<PhpVersion[]>([])
const selected = ref<string>('')
const status = ref<PhpExtStatus | null>(null)
const loading = ref(false)
const loadError = ref('')
const busy = ref<string | null>(null)
const actionError = ref('')
const notice = ref('')
const filter = ref('')

const toggleable = computed<PhpExt[]>(() => {
  const q = filter.value.trim().toLowerCase()
  return (status.value?.extensions ?? []).filter((e) => e.available && (!q || e.name.includes(q)))
})

const builtin = computed<string[]>(() =>
  (status.value?.extensions ?? []).filter((e) => e.builtin).map((e) => e.name),
)

const enabledCount = computed<number>(() => (status.value?.extensions ?? []).filter((e) => e.available && e.loaded).length)

async function load(): Promise<void> {
  loading.value = true
  loadError.value = ''
  try {
    status.value = await invoke<PhpExtStatus>('php_ext_status', { version: selected.value || null })
    if (!selected.value) selected.value = status.value.version
  } catch (err) {
    status.value = null
    loadError.value = String(err)
  } finally {
    loading.value = false
  }
}

async function chooseVersion(v: string): Promise<void> {
  selected.value = v
  actionError.value = ''
  notice.value = ''
  await load()
}

async function toggle(ext: PhpExt): Promise<void> {
  if (busy.value) return
  busy.value = ext.name
  actionError.value = ''
  notice.value = ''
  try {
    status.value = await invoke<PhpExtStatus>('php_ext_set', {
      version: selected.value || null,
      name: ext.name,
      enable: !ext.loaded,
    })
    notice.value = `${ext.name} ${ext.loaded ? 'disabled' : 'enabled'}. Restart running PHP projects to pick it up.`
  } catch (err) {
    actionError.value = String(err)
  } finally {
    busy.value = null
  }
}

async function fixHttps(): Promise<void> {
  if (busy.value) return
  busy.value = '__https'
  actionError.value = ''
  notice.value = ''
  try {
    status.value = await invoke<PhpExtStatus>('php_ext_fix_https', { version: selected.value || null })
    notice.value = 'HTTPS certificates set. Restart running PHP projects to pick them up.'
  } catch (err) {
    actionError.value = String(err)
  } finally {
    busy.value = null
  }
}

onMounted(async () => {
  try {
    versions.value = await invoke<PhpVersion[]>('php_list_installed')
    selected.value = versions.value.find((v) => v.active)?.version ?? ''
  } catch {
    versions.value = []
  }
  await load()
})
</script>

<template>
  <div class="px">
    <div class="px-head">
      <div class="px-title">PHP EXTENSIONS</div>
      <select
        v-if="versions.length > 1"
        id="px-version"
        class="px-select"
        :value="selected"
        aria-label="PHP version"
        @change="chooseVersion(($event.target as HTMLSelectElement).value)"
      >
        <option v-for="v in versions" :key="v.version" :value="v.version">
          PHP {{ v.version }}{{ v.active ? ' (active)' : '' }}
        </option>
      </select>
    </div>
    <p class="px-hint">
      Extensions are set in php.ini for one PHP version, so a change applies to every project and terminal that uses
      that version. Servel backs up php.ini first and undoes the change if PHP no longer starts.
    </p>

    <p v-if="loading && !status" class="px-hint">Reading PHP…</p>
    <p v-else-if="loadError" class="px-error">{{ loadError }}</p>

    <template v-if="status">
      <div class="px-meta">
        <span>PHP {{ status.version }}</span>
        <span :title="status.iniPath">php.ini: {{ status.iniPath }}{{ status.iniExists ? '' : ' (created on first change)' }}</span>
      </div>

      <div v-if="status.https" class="px-https" :class="{ 'px-https--ok': status.https.ok }">
        <div class="px-https__text">
          <strong>HTTPS certificates</strong>
          <span v-if="status.https.ok" :title="status.https.cainfo ?? ''">Set: {{ status.https.cainfo }}</span>
          <span v-else>
            Not set. PHP on Windows ships without a certificate list, so HTTPS calls from curl fail with error 60.
          </span>
        </div>
        <button
          class="px-btn"
          :class="{ 'px-btn--accent': !status.https.ok }"
          :disabled="busy !== null"
          @click="fixHttps"
        >{{ busy === '__https' ? 'Setting…' : status.https.ok ? 'Set again' : 'Fix HTTPS certificates' }}</button>
      </div>

      <div class="px-toolbar">
        <input
          id="px-filter"
          v-model="filter"
          class="px-input"
          placeholder="Filter extensions"
          aria-label="Filter extensions"
          spellcheck="false"
        />
        <span class="px-count">{{ enabledCount }} of {{ status.extensions.filter((e) => e.available).length }} enabled</span>
      </div>

      <div class="px-grid">
        <div v-for="ext in toggleable" :key="ext.name" class="px-item">
          <span class="px-name">{{ ext.name }}</span>
          <button
            class="px-toggle"
            :class="{ 'px-toggle--on': ext.loaded }"
            role="switch"
            :aria-checked="ext.loaded"
            :aria-label="`${ext.loaded ? 'Disable' : 'Enable'} ${ext.name}`"
            :disabled="busy !== null"
            @click="toggle(ext)"
          >
            <span class="px-knob"></span>
          </button>
        </div>
        <p v-if="toggleable.length === 0" class="px-hint">No extension matches "{{ filter }}".</p>
      </div>

      <p v-if="notice" class="px-notice">{{ notice }}</p>
      <p v-if="actionError" class="px-error">{{ actionError }}</p>

      <details v-if="builtin.length" class="px-builtin">
        <summary>{{ builtin.length }} built into this PHP (always on)</summary>
        <p>{{ builtin.join(', ') }}</p>
      </details>
      <p class="px-hint">
        Need redis, xdebug or imagick? Download the DLL that matches PHP {{ status.version }} into
        <code>{{ status.extensionDir }}</code>, then it appears here to enable.
      </p>
    </template>
  </div>
</template>

<style scoped>
.px {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.px-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  flex-wrap: wrap;
}

.px-title {
  font-family: var(--font-sans);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--dim);
}

.px-hint,
.px-notice,
.px-error {
  font-family: var(--font-sans);
  font-size: 12px;
  line-height: 1.55;
  color: var(--muted);
  margin: 0;
}

.px-hint code {
  font-family: var(--font-mono);
  font-size: 11px;
  overflow-wrap: anywhere;
}

.px-notice {
  color: var(--green);
}

.px-error {
  color: var(--red);
  overflow-wrap: anywhere;
}

.px-meta {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-4);
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
  min-width: 0;
}

.px-meta span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}

.px-https {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  flex-wrap: wrap;
  padding: var(--space-3) var(--space-4);
  border: 1px solid color-mix(in srgb, var(--amber) 40%, transparent);
  background: color-mix(in srgb, var(--amber) 8%, transparent);
  border-radius: var(--radius-md);
}

.px-https--ok {
  border-color: var(--border);
  background: var(--surface);
}

.px-https__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1 1 260px;
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--muted);
}

.px-https__text strong {
  color: var(--text);
  font-weight: 600;
}

.px-https__text span {
  overflow-wrap: anywhere;
}

.px-toolbar {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.px-input,
.px-select {
  font-family: var(--font-sans);
  font-size: 12px;
  padding: 5px 10px;
  background: var(--bg);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-xs);
  color: var(--text);
}

.px-input {
  flex: 0 1 240px;
  min-width: 0;
}

.px-count {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--dim);
}

.px-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
  gap: var(--space-2);
}

.px-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: 7px 10px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  min-width: 0;
}

.px-name {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
}

.px-toggle {
  position: relative;
  flex: none;
  width: 32px;
  height: 18px;
  border-radius: var(--radius-md);
  background: var(--surface2);
  border: 1px solid var(--border);
  cursor: pointer;
  padding: 0;
  transition: background 0.15s, border-color 0.15s;
}

.px-toggle:disabled {
  cursor: wait;
  opacity: 0.6;
}

.px-toggle--on {
  background: color-mix(in srgb, var(--accent) 35%, transparent);
  border-color: color-mix(in srgb, var(--accent) 60%, transparent);
}

.px-knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--muted);
  transition: transform 0.15s, background 0.15s;
}

.px-toggle--on .px-knob {
  transform: translateX(14px);
  background: var(--accent);
}

.px-btn {
  font-family: var(--font-sans);
  font-size: 11px;
  padding: 4px 12px;
  background: var(--surface2);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-xs);
  color: var(--text);
  cursor: pointer;
  white-space: nowrap;
}

.px-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.px-btn--accent {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  color: var(--accent);
}

.px-btn:focus-visible,
.px-toggle:focus-visible,
.px-input:focus-visible,
.px-select:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.px-builtin {
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--muted);
}

.px-builtin summary {
  cursor: pointer;
}

.px-builtin p {
  margin: var(--space-2) 0 0;
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.6;
  color: var(--dim);
}
</style>
