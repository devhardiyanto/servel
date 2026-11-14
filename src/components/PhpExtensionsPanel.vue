<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
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
  canInstall: boolean
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
const installName = ref('')

/** Extension PECL yang sering dipakai; nama lain boleh diketik. */
const SUGGESTED = ['redis', 'xdebug', 'imagick', 'mongodb', 'apcu', 'igbinary', 'pcov', 'memcached', 'yaml']

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

async function installExt(): Promise<void> {
  const name = installName.value.trim().toLowerCase()
  if (busy.value || !name) return
  if (!/^[a-z0-9_]+$/.test(name)) {
    actionError.value = 'Extension names use only letters, digits and underscores, e.g. redis.'
    return
  }
  busy.value = '__install'
  actionError.value = ''
  notice.value = ''
  try {
    status.value = await invoke<PhpExtStatus>('php_ext_install', { version: selected.value || null, name })
    notice.value = `${name} installed and enabled. Restart running PHP projects to pick it up.`
    installName.value = ''
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
        <UiButton
          :variant="status.https.ok ? 'default' : 'accent'"
          :disabled="busy !== null"
          @click="fixHttps"
        >{{ busy === '__https' ? 'Setting…' : status.https.ok ? 'Set again' : 'Fix HTTPS certificates' }}</UiButton>
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
          <UiToggle
            size="sm"
            :on="ext.loaded"
            :label="`${ext.loaded ? 'Disable' : 'Enable'} ${ext.name}`"
            :disabled="busy !== null"
            @click="toggle(ext)"
          />
        </div>
        <p v-if="toggleable.length === 0" class="px-hint">No extension matches "{{ filter }}".</p>
      </div>

      <p v-if="notice" class="px-notice">{{ notice }}</p>
      <p v-if="actionError" class="px-error">{{ actionError }}</p>

      <details v-if="builtin.length" class="px-builtin">
        <summary>{{ builtin.length }} built into this PHP (always on)</summary>
        <p>{{ builtin.join(', ') }}</p>
      </details>
      <div v-if="status.canInstall" class="px-install">
        <label for="px-install-name" class="px-install__label">Install another extension</label>
        <div class="px-install__row">
          <input
            id="px-install-name"
            v-model="installName"
            class="px-input"
            list="px-install-list"
            placeholder="redis, xdebug, imagick…"
            spellcheck="false"
            :disabled="busy !== null"
            @keydown.enter="installExt"
          />
          <datalist id="px-install-list">
            <option v-for="n in SUGGESTED" :key="n" :value="n" />
          </datalist>
          <UiButton variant="accent" :disabled="busy !== null || !installName.trim()" @click="installExt">
            {{ busy === '__install' ? 'Downloading…' : 'Download & enable' }}
          </UiButton>
        </div>
        <p class="px-hint">
          Downloads the build that matches PHP {{ status.version }} from downloads.php.net, puts it in
          <code>{{ status.extensionDir }}</code> and turns it on. If PHP rejects it, the file is removed and php.ini
          is put back.
        </p>
      </div>
      <p v-else class="px-hint">
        Need redis, xdebug or imagick? Install it with <code>pecl install</code> for PHP {{ status.version }}; it
        then appears here to enable.
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
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.12em;
  color: var(--faint);
}

.px-hint,
.px-notice,
.px-error {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  line-height: 1.55;
  color: var(--muted);
  margin: 0;
}

.px-hint code {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
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
  font-size: var(--text-xs);
  color: var(--faint);
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
  font-size: var(--text-xs);
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
  font-size: var(--text-xs);
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
  font-size: var(--text-xs);
  color: var(--faint);
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
  font-size: var(--text-xs);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
}

.px-input:focus-visible,
.px-select:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.px-install {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-top: var(--space-3);
  border-top: 1px solid var(--border);
}

.px-install__label {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--text);
}

.px-install__row {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}

.px-builtin {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--muted);
}

.px-builtin summary {
  cursor: pointer;
}

.px-builtin p {
  margin: var(--space-2) 0 0;
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--faint);
}
</style>
