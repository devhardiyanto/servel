<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import UiToggle from '@/components/ui/UiToggle.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import { ref, computed, onMounted, watch } from 'vue'
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
const props = defineProps<{ focusVersion?: string | null }>()

/** HTTPS fix and extension download: one at a time, and not during toggles. */
const busy = ref<string | null>(null)
/** Extensions being written and checked with php -m; each row shows its own state. */
const pending = ref(new Set<string>())
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

/**
 * Extension yang ditolak PHP → php.ini dikembalikan.
 * Ditampilkan sebagai banner: penyebab, bahwa tidak ada yang berubah, dan
 * cara memperbaikinya.
 */
const rolledBackExt = ref('')
const rolledBack = computed(() => !!rolledBackExt.value && /put back as it was/i.test(actionError.value))
const rollbackCause = computed(() => actionError.value.replace(/\.?\s*php\.ini was put back as it was\.?$/i, '').trim())

async function toggle(ext: PhpExt): Promise<void> {
  if (busy.value || pending.value.has(ext.name)) return
  pending.value = new Set(pending.value).add(ext.name)
  const enable = !ext.loaded
  rolledBackExt.value = ''
  actionError.value = ''
  notice.value = ''
  try {
    // The backend serialises writes, so other toggles can be clicked meanwhile.
    status.value = await invoke<PhpExtStatus>('php_ext_set', {
      version: selected.value || null,
      name: ext.name,
      enable,
    })
    notice.value = `${ext.name} ${enable ? 'enabled' : 'disabled'}. Restart running PHP projects to pick it up.`
  } catch (err) {
    actionError.value = String(err)
    rolledBackExt.value = enable ? ext.name : ''
  } finally {
    const next = new Set(pending.value)
    next.delete(ext.name)
    pending.value = next
  }
}

async function fixHttps(): Promise<void> {
  if (busy.value || pending.value.size) return
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
  if (busy.value || pending.value.size || !name) return
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

async function loadVersions(): Promise<void> {
  try {
    versions.value = await invoke<PhpVersion[]>('php_list_installed')
  } catch {
    versions.value = []
  }
}

onMounted(async () => {
  await loadVersions()
  selected.value = props.focusVersion && versions.value.some((v) => v.version === props.focusVersion)
    ? props.focusVersion
    : (versions.value.find((v) => v.active)?.version ?? '')
  await load()
})

// Runtimes points the panel at the version just installed, made global or
// used in a project, so the extensions shown are the ones that change.
watch(
  () => props.focusVersion,
  async (v) => {
    if (!v || v === selected.value) return
    await loadVersions()
    if (versions.value.some((x) => x.version === v)) await chooseVersion(v)
  },
)
</script>

<template>
  <div class="px">
    <div class="px-head">
      <div class="px-title">PHP EXTENSIONS</div>
      <UiInput
        v-if="versions.length > 1"
        as="select"
        width="170px"
        aria-label="PHP version"
        :model-value="selected ?? ''"
        :options="versions.map((v) => ({ value: v.version, label: `PHP ${v.version}${v.active ? ' (active)' : ''}` }))"
        @update:model-value="chooseVersion"
      />
    </div>
    <p class="px-hint">
      Extensions are set in php.ini for one PHP version, so a change applies to every project and terminal that uses
      that version. Kova backs up php.ini first and undoes the change if PHP no longer starts.
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
          :disabled="busy !== null || pending.size > 0"
          @click="fixHttps"
        >{{ busy === '__https' ? 'Setting…' : status.https.ok ? 'Set again' : 'Fix HTTPS certificates' }}</UiButton>
      </div>

      <div class="px-toolbar">
        <UiInput v-model="filter" type="search" width="220px" placeholder="Filter extensions" aria-label="Filter extensions" />
        <span class="px-count">{{ enabledCount }} of {{ status.extensions.filter((e) => e.available).length }} enabled</span>
      </div>

      <div class="px-grid">
        <div v-for="ext in toggleable" :key="ext.name" class="px-item">
          <span class="px-name">{{ ext.name }}</span>
          <span v-if="pending.has(ext.name)" class="px-checking" aria-live="polite">checking…</span>
          <UiToggle
            size="sm"
            :on="ext.loaded"
            :label="`${ext.loaded ? 'Disable' : 'Enable'} ${ext.name}`"
            :disabled="busy !== null || pending.has(ext.name)"
            @click="toggle(ext)"
          />
        </div>
        <p v-if="toggleable.length === 0" class="px-hint">No extension matches "{{ filter }}".</p>
      </div>

      <p v-if="notice" class="px-notice">{{ notice }}</p>
      <UiBanner
        v-if="rolledBack"
        tone="err"
        :title="`${rolledBackExt} was not turned on. Nothing changed.`"
      >
        {{ rollbackCause }}. Kova put php.ini back as it was, so PHP and your projects work as before.
        To use {{ rolledBackExt }}, get a build made for this PHP version (same version, thread safety and
        architecture) and install it below.
      </UiBanner>
      <p v-else-if="actionError" class="px-error">{{ actionError }}</p>

      <details v-if="builtin.length" class="px-builtin">
        <summary>{{ builtin.length }} built into this PHP (always on)</summary>
        <p>{{ builtin.join(', ') }}</p>
      </details>
      <div v-if="status.canInstall" class="px-install">
        <span class="px-install__label">Install another extension</span>
        <div class="px-install__row">
          <UiInput
            v-model="installName"
            width="220px"
            list="px-install-list"
            aria-label="Extension to install"
            placeholder="redis, xdebug, imagick…"
            :disabled="busy !== null || pending.size > 0"
            @keydown.enter="installExt"
          />
          <datalist id="px-install-list">
            <option v-for="n in SUGGESTED" :key="n" :value="n" />
          </datalist>
          <UiButton variant="accent" :disabled="busy !== null || pending.size > 0 || !installName.trim()" @click="installExt">
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

.px-checking { margin-left: auto; font-size: var(--text-xs); color: var(--amber); }
.px-name {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
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
