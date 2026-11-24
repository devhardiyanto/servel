<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import UiIcon from '@/components/ui/UiIcon.vue'
import UiInput from '@/components/ui/UiInput.vue'
import { onMounted, computed, inject, ref } from 'vue'
import { useLogs } from '@/composables/useLogs'
import { useServices } from '@/composables/useServices'
import { useConfig } from '@/composables/useConfig'
import { useEngineStatus } from '@/composables/useEngineStatus'
import { SetViewKey } from '@/types/navigation'
import LogTail from '@/components/LogTail.vue'
import ServiceRow from '@/components/ServiceRow.vue'
import ServiceDetail from '@/components/ServiceDetail.vue'
import PanelSplitter from '@/components/ui/PanelSplitter.vue'
import { usePanelWidth } from '@/composables/usePanelWidth'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import UiBanner from '@/components/ui/UiBanner.vue'
import UiStatusNotice from '@/components/ui/UiStatusNotice.vue'
import UiModal from '@/components/ui/UiModal.vue'
import AddServiceModal from '@/components/AddServiceModal.vue'
import ServicePortModal from '@/components/ServicePortModal.vue'
import { invoke } from '@tauri-apps/api/core'
import { useToast } from '@/composables/useToast'
import type { ServiceDef } from '@/types/service'

const SKELETON_ROW_COUNT = 9

const setView = inject(SetViewKey)!

// Panel Connect: satu service terbuka sekaligus.
const connectId = ref<string | null>(null)
// Lebar panel service bisa diseret; seret habis ke kanan = tutup panel.
const SVC_PANEL = { default: 320, min: 280, max: 480 }
const svcPanel = usePanelWidth('services', SVC_PANEL)
function toggleConnect(id: string): void {
  connectId.value = connectId.value === id ? null : id
}
const connectDef = computed(() => definitions.value.find((d) => d.id === connectId.value) ?? null)

const { lines: logLines } = useLogs()
const {
  engineStatus,
  engine,
  engineLabel,
  wslcAvailable,
  lastError: engineLastError,
  refresh: refreshEngine,
} = useEngineStatus()

const refreshingEngine = ref(false)
async function handleEngineRefresh(): Promise<void> {
  if (refreshingEngine.value) return
  refreshingEngine.value = true
  try {
    await refreshEngine()
  } finally {
    refreshingEngine.value = false
  }
}

// Docker mati tapi WSLC tersedia → tawarkan pindah engine satu klik.
const canSwitchToWslc = computed(() => engine.value === 'docker' && wslcAvailable.value)
async function switchToWslc(): Promise<void> {
  if (refreshingEngine.value) return
  refreshingEngine.value = true
  try {
    await useConfig().setContainerRuntime('wslc')
    await refreshEngine()
  } finally {
    refreshingEngine.value = false
  }
}

const {
  uiState,
  coreServices,
  additionalServices,
  customServices,
  runningCount,
  selectedCount,
  selectedIds,
  hasStarting,
  hasStopping,
  definitions,
  serviceError,
  portConflicts,
  load: loadServices,
  toggle,
  setSelectedIds,
  reconcileSelectedWithRunning,
  applyProfile,
  start,
  stopAll,
  dismissPortConflicts,
  startIgnoringConflicts,
} = useServices()

const profiles = computed(() => useConfig().config.value.profiles)
const activeProfileId = computed(() => useConfig().config.value.activeProfileId)
const activeProfileName = computed(() => profiles.value.find((p) => p.id === activeProfileId.value)?.name ?? 'this profile')

/**
 * Tombol utama memakai nama profil selama pilihan sama dengan isi profil
 * aktif ("Start Acme · 5"). Begitu switch diubah, tombol kembali menjadi
 * "Start selected".
 */
const selectionIsProfile = computed(() => {
  const p = profiles.value.find((x) => x.id === activeProfileId.value)
  if (!p || p.serviceIds.length === 0) return false
  const sel = new Set(selectedIds.value)
  return sel.size === p.serviceIds.length && p.serviceIds.every((id) => sel.has(id))
})
const startingCount = computed(() => Object.values(uiState.value).filter((s) => s.status === 'starting').length)
/** Profile whose empty-state banner the user closed with "Show all services". */
const emptyDismissed = ref<string | null>(null)
const startLabel = computed(() =>
  selectionIsProfile.value ? `Start ${activeProfileName.value}` : 'Start selected',
)

// Start / Retry dari baris service yang dipilih tetapi tidak berjalan.
function startOne(id: string): void {
  void start([id])
}

// --- Menu ⋯ per service dan custom service (Flow J) -------------------------
const toast = useToast()
const services = useServices()
const addOpen = ref(false)
const editDef = ref<ServiceDef | null>(null)
const portDef = ref<ServiceDef | null>(null)
const deleteDef = ref<ServiceDef | null>(null)
const removeDef = ref<ServiceDef | null>(null)
const backupFirst = ref(true)
const removeData = ref(false)
const working = ref(false)
const detailTab = ref<{ id: string; tab: 'connect' | 'data' | 'logs'; n: number } | null>(null)
const canBackup = (d: ServiceDef | null) => d?.id === 'mysql' || d?.id === 'postgres'

async function onServiceMenu(id: string, action: string): Promise<void> {
  const def = definitions.value.find((d) => d.id === id)
  if (!def) return
  try {
    if (action === 'restart') await services.restart(id)
    else if (action === 'shell') await invoke('service_shell', { id })
    else if (action === 'port') portDef.value = def
    else if (action === 'logs') {
      connectId.value = id
      detailTab.value = { id, tab: 'logs', n: Date.now() }
    } else if (action === 'edit') {
      editDef.value = def
      addOpen.value = true
    } else if (action === 'delete-data') {
      backupFirst.value = canBackup(def)
      deleteDef.value = def
    } else if (action === 'remove') {
      removeData.value = false
      removeDef.value = def
    }
  } catch (err) {
    toast.show(String(err), { tone: 'info' })
  }
}

async function confirmDeleteData(): Promise<void> {
  const def = deleteDef.value
  if (!def) return
  working.value = true
  try {
    const saved = await invoke<string | null>('service_delete_data', { id: def.id, backupFirst: backupFirst.value && canBackup(def) })
    deleteDef.value = null
    toast.show(saved ? `${def.name} data deleted. Backup: ${saved}` : `${def.name} data deleted. It starts empty.`, { timed: true })
  } catch (err) {
    toast.show(`Could not delete ${def.name} data: ${String(err)}`, { tone: 'info' })
  } finally {
    working.value = false
  }
}

async function confirmRemove(): Promise<void> {
  const def = removeDef.value
  if (!def) return
  working.value = true
  try {
    await invoke('service_remove', { id: def.id, deleteData: removeData.value })
    if (connectId.value === def.id) connectId.value = null
    await useConfig().removeCustomService(def.id)
    await services.reloadDefinitions()
    removeDef.value = null
    toast.show(`Removed ${def.name}${removeData.value ? ' and its data' : ''}`)
  } catch (err) {
    toast.show(`Could not remove ${def.name}: ${String(err)}`, { tone: 'info' })
  } finally {
    working.value = false
  }
}

function onCustomSaved(id: string): void {
  connectId.value = id
}

// Banner error service: Retry menyalakan lagi service yang gagal.
const runningRamMb = computed(() =>
  Object.values(uiState.value)
    .filter((s) => s.status === 'running' && s.memMb != null)
    .reduce((sum, s) => sum + (s.memMb ?? 0), 0),
)
const failedIds = computed(() => Object.values(uiState.value).filter((s) => s.error && s.status !== 'running').map((s) => s.id))
function retryFailed(): void {
  serviceError.value = null
  if (failedIds.value.length) void start(failedIds.value)
}

function onProfileChange(id: string): void {
  if (id) applyProfile(id)
}

onMounted(async () => {
  await loadServices()
  await useConfig().load()

  const cfg = useConfig().config.value

  if (cfg.selectedServiceIds.length > 0) {
    setSelectedIds(cfg.selectedServiceIds)
  }

  // Rekonsiliasi: container yang auto-restart (restart: unless-stopped) setelah
  // reboot tampil running → switch harus ON walau tidak ada di saved config.
  // Dipanggil SETELAH config loaded + setSelectedIds; persist watch akan menulis
  // ulang selection dari running state (menyembuhkan config yang hilang).
  reconcileSelectedWithRunning()

})


/** "Choose services" pada profil kosong membawa fokus ke switch pertama. */
function focusFirstSwitch(): void {
  const el = document.querySelector<HTMLElement>('.services-table [role="switch"]')
  el?.scrollIntoView({ block: 'nearest' })
  el?.focus()
}
</script>

<template>
  <div class="view view-dashboard">
    <div class="dashboard-content">
      <header class="sw-header">
        <h1 class="sw-title">Services</h1>
        <span class="sw-counter">{{ runningCount }}/{{ definitions.length }}</span>
        <div v-if="profiles.length > 0" class="sw-profile">
          <span class="sw-profile__label">profile</span>
          <UiInput
            as="select"
            width="160px"
            aria-label="Active profile"
            :model-value="activeProfileId ?? ''"
            :options="profiles.map((p) => ({ value: p.id, label: p.name }))"
            @update:model-value="onProfileChange"
          />
        </div>
        <div class="sw-actions">
          <UiButton size="md" @click="editDef = null; addOpen = true"><UiIcon name="plus" />Add service</UiButton>
          <UiButton
            size="md"
            :disabled="hasStopping || runningCount === 0"
            @click="stopAll"
          >
            <UiIcon name="stop" />Stop all
          </UiButton>
          <UiButton v-if="hasStarting" variant="primary" size="md" loading>
            Starting {{ startingCount }} {{ startingCount === 1 ? 'service' : 'services' }}…
          </UiButton>
          <UiButton
            v-else
            variant="primary"
            size="md"
            :disabled="selectedCount === 0"
            :aria-label="`${startLabel}, ${selectedCount} ${selectedCount === 1 ? 'service' : 'services'}`"
            @click="start()"
          >
            <UiIcon name="play" />{{ startLabel }}<span v-if="selectedCount > 0" class="sw-start-count">{{ selectedCount }}</span>
          </UiButton>
        </div>
      </header>
    <UiBanner
      v-if="engineStatus === 'down' || engineStatus === 'missing'"
      tone="err"
      :title="`${engineLabel} is ${engineStatus === 'missing' ? 'not installed' : 'not responding'}`"
    >
      <template v-if="engine === 'wslc'">Run <code>wsl --update</code>, then Retry.</template>
      <template v-else-if="canSwitchToWslc">Start Docker Desktop, or switch to the built-in WSL containers.</template>
      <template v-else>{{ engineStatus === 'missing' ? 'Install Docker Desktop' : 'Start Docker Desktop' }} to use services.</template>
      <span v-if="engineLastError"> ({{ engineLastError }})</span>
      Kova can't read service status. Data in the servel_* volumes is not affected.
      <template #actions>
        <UiButton variant="quiet" @click="setView('settings')">Engine settings</UiButton>
        <UiButton v-if="canSwitchToWslc" variant="accent" :disabled="refreshingEngine" @click="switchToWslc">
          Use WSL containers
        </UiButton>
        <UiButton :loading="refreshingEngine" @click="handleEngineRefresh">Retry</UiButton>
      </template>
    </UiBanner>

    <UiBanner
      v-if="portConflicts.length > 0"
      tone="warn"
      :title="portConflicts.length === 1 ? `Port ${portConflicts[0].port} is taken by another program` : 'Ports are taken by other programs'"
    >
      <template v-for="(c, i) in portConflicts" :key="c.serviceId + ':' + c.port">
        {{ c.serviceName }} ({{ c.port }})<span v-if="i < portConflicts.length - 1">, </span>
      </template>
      can't start on {{ portConflicts.length === 1 ? 'it' : 'them' }}.
      <template #actions>
        <UiButton
          v-if="portConflicts.length === 1"
          variant="accent"
          @click="portDef = definitions.find((d) => d.id === portConflicts[0].serviceId) ?? null; dismissPortConflicts()"
        >Change port…</UiButton>
        <UiButton @click="startIgnoringConflicts">Start anyway</UiButton>
        <UiButton variant="quiet" @click="dismissPortConflicts">Close</UiButton>
      </template>
    </UiBanner>

    <UiStatusNotice
      v-if="serviceError"
      tone="err"
      title="A service did not start"
      :detail="serviceError"
      :retryable="failedIds.length > 0"
      dismissible
      @retry="retryFailed"
      @dismiss="serviceError = null"
    />

    <section class="services-wrap">
      <UiBanner
        v-if="definitions.length > 0 && selectedCount === 0 && emptyDismissed !== (activeProfileId ?? '') && engineStatus !== 'down' && engineStatus !== 'missing'"
        class="sw-empty"
        tone="info"
        :title="`No services in ${activeProfileName} yet`"
      >
        Turn on the services this profile starts with the switches below. Projects that use {{ activeProfileName }} can start them together with the project.
        <template #actions>
          <UiButton variant="accent" @click="focusFirstSwitch">Choose services</UiButton>
          <UiButton variant="quiet" @click="emptyDismissed = activeProfileId ?? ''">Not now</UiButton>
        </template>
      </UiBanner>
      <div class="services-table">
        <template v-if="coreServices.length > 0">
          <div class="svc-group-label">core</div>
          <template v-for="def in coreServices" :key="def.id">
            <ServiceRow
              :def="def"
              :state="uiState[def.id]"
              :connect-open="connectId === def.id"
              :unknown="engineStatus === 'down' || engineStatus === 'missing'"
              @toggle="toggle"
              @connect="toggleConnect"
              @start="startOne"
              @menu="onServiceMenu"
            />
          </template>
        </template>
        <template v-if="additionalServices.length > 0">
          <div class="svc-group-label">additional</div>
          <template v-for="def in additionalServices" :key="def.id">
            <ServiceRow
              :def="def"
              :state="uiState[def.id]"
              :connect-open="connectId === def.id"
              :unknown="engineStatus === 'down' || engineStatus === 'missing'"
              @toggle="toggle"
              @connect="toggleConnect"
              @start="startOne"
              @menu="onServiceMenu"
            />
          </template>
        </template>
        <template v-if="customServices.length > 0">
          <div class="svc-group-label">custom</div>
          <template v-for="def in customServices" :key="def.id">
            <ServiceRow
              :def="def"
              :state="uiState[def.id]"
              :connect-open="connectId === def.id"
              :unknown="engineStatus === 'down' || engineStatus === 'missing'"
              @toggle="toggle"
              @connect="toggleConnect"
              @start="startOne"
              @menu="onServiceMenu"
            />
          </template>
        </template>
        <template v-if="definitions.length === 0 && !serviceError">
          <p class="svc-loading" role="status">Asking {{ engineLabel }} for status…</p>
          <div
            v-for="i in SKELETON_ROW_COUNT"
            :key="`skeleton-${i}`"
            class="svc-row-skeleton"
            aria-busy="true"
            aria-label="Loading service"
          >
            <SkeletonBox width="28px" height="15px" radius="8px" />
            <SkeletonBox width="18px" height="18px" radius="4px" />
            <div class="svc-row-skeleton__id">
              <SkeletonBox width="60%" height="12px" />
              <SkeletonBox width="40%" height="9px" />
            </div>
            <SkeletonBox width="70px" height="11px" />
            <SkeletonBox width="50px" height="11px" />
            <SkeletonBox variant="chip" width="40px" height="14px" />
          </div>
        </template>
      </div>
      <div v-if="definitions.length > 0" class="sw-summary">
        <span>{{ runningCount }} running<template v-if="runningRamMb > 0"> · {{ runningRamMb >= 1024 ? `${(runningRamMb / 1024).toFixed(1)} GB` : `${Math.round(runningRamMb)} MB` }}</template></span>
        <span class="sw-sp"></span>
        <span>Engine: {{ engineLabel }}</span>
      </div>
    </section>

    <section class="log-wrap">
      <div class="log-wrap__bar">
        <span class="log-wrap__label">LOGS</span>
        <UiButton variant="quiet" @click="setView('logs')">Open logs</UiButton>
      </div>
      <LogTail :lines="logLines" max-height="148px" />
    </section>
    </div>

    <PanelSplitter
      v-if="connectDef"
      v-model:width="svcPanel.width.value"
      v-bind="{ min: SVC_PANEL.min, max: SVC_PANEL.max, defaultWidth: SVC_PANEL.default }"
      controls="svc-detail"
      :label="`Resize ${connectDef.name} details`"
      @collapse="connectId = null"
    />
    <aside
      v-if="connectDef"
      id="svc-detail"
      class="svc-detail"
      :aria-label="`${connectDef.name} details`"
      :style="{ width: `${svcPanel.width.value}px` }"
    >
      <ServiceDetail :key="connectDef.id" :def="connectDef" :focus-tab="detailTab?.id === connectDef.id ? detailTab : null" @close="connectId = null" />
    </aside>

    <AddServiceModal :open="addOpen" :edit="editDef" @close="addOpen = false; editDef = null" @saved="onCustomSaved" />
    <ServicePortModal v-if="portDef" :open="!!portDef" :def="portDef" @close="portDef = null" />
    <UiModal :open="!!deleteDef" :title="`Delete ${deleteDef?.name ?? ''} data?`" size="sm" danger @close="deleteDef = null">
      <p class="sw-confirm">{{ deleteDef?.name }} stops, its volume{{ (deleteDef?.volumes?.length ?? 0) > 1 ? 's are' : ' is' }} deleted, and it starts again empty. Everything stored in it is gone. This cannot be undone.</p>
      <ul class="sw-confirm sw-mono"><li v-for="v in deleteDef?.volumes ?? []" :key="v">{{ v.split(':')[0] }}</li></ul>
      <label v-if="canBackup(deleteDef)" class="sw-check"><input v-model="backupFirst" type="checkbox"> Save a .sql backup to Downloads first</label>
      <template #footer>
        <UiButton size="md" data-autofocus @click="deleteDef = null">Cancel</UiButton>
        <UiButton variant="danger" size="md" :loading="working" @click="confirmDeleteData">Delete data</UiButton>
      </template>
    </UiModal>
    <UiModal :open="!!removeDef" :title="`Remove ${removeDef?.name ?? ''}?`" size="sm" danger @close="removeDef = null">
      <p class="sw-confirm">Kova stops {{ removeDef?.name }} and removes its container and its definition. Profiles stop listing it.</p>
      <label v-if="(removeDef?.volumes?.length ?? 0) > 0" class="sw-check"><input v-model="removeData" type="checkbox"> Also delete its data ({{ removeDef?.volumes?.map((v) => v.split(':')[0]).join(', ') }})</label>
      <template #footer>
        <UiButton size="md" data-autofocus @click="removeDef = null">Cancel</UiButton>
        <UiButton variant="danger" size="md" :loading="working" @click="confirmRemove">Remove service</UiButton>
      </template>
    </UiModal>
  </div>
</template>

<style scoped>
.sw-confirm { margin: 0 0 var(--space-3); color: var(--muted); font-size: var(--text-sm); }
.sw-mono { font-family: var(--font-mono); font-size: var(--text-xs); padding-left: var(--space-4); }
.sw-check { display: flex; align-items: center; gap: var(--space-2); font-size: var(--text-sm); }

.view-dashboard {
  display: flex;
  flex-direction: row;
  height: 100%;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
}

.dashboard-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  padding: 20px 28px;
  gap: var(--space-4);
  min-height: 0;
  overflow: hidden;
}

.services-wrap {
  margin: 0;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

/* Judul layar di luar kartu tabel. */
.sw-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2) var(--space-3);
  padding: 0 0 var(--space-1);
  flex-shrink: 0;
}

.sw-title {
  margin: 0;
  font-family: var(--font-sans);
  font-size: var(--text-xl);
  font-weight: 600;
  letter-spacing: -0.015em;
  color: var(--text);
}

.sw-counter {
  font: 500 var(--text-sm) var(--font-mono);
  color: var(--muted);
}

.sw-start-count {
  margin-left: 6px;
  padding: 0 6px;
  border-radius: var(--radius-xs);
  background: color-mix(in srgb, var(--bg) 22%, transparent);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
}

.sw-profile {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-right: auto;
  margin-left: var(--space-3);
}

.sw-profile__label {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--muted);
}

.sw-summary {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-5);
  border-top: 1px solid var(--border);
  font-size: var(--text-xs);
  color: var(--muted);
}
.sw-sp { flex: 1; }

.sw-actions {
  display: flex;
  gap: var(--space-2);
  margin-left: auto;
}

.services-table {
  container-type: inline-size;
  padding: var(--space-2) 0;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.svc-group-label {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  font-weight: 500;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--faint);
  padding: var(--space-2) var(--space-4) var(--space-1);
}

.svc-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  min-height: 60px;
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--faint);
}

/* Grid sama dengan .svc-row supaya isi asli tidak bergeser saat muncul. */
.svc-row-skeleton {
  display: grid;
  grid-template-columns: 36px 24px 1fr 100px 68px 80px 76px 32px;
  align-items: center;
  gap: var(--space-3);
  padding: 5px var(--space-4);
}

.svc-row-skeleton__id {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.log-wrap {
  margin: 0;
  height: 184px;
  flex-shrink: 0;
}

.log-wrap__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--space-2) 4px;
}

.log-wrap__label {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--faint);
  text-transform: uppercase;
}

.svc-detail {
  flex-shrink: 0;
  max-width: 55%;
  overflow-y: auto;
  padding: var(--space-5);
  background: var(--bg-sunken);
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.svc-detail__head { display: flex; align-items: center; justify-content: space-between; }
.svc-detail__name { font-size: var(--text-lg); font-weight: 600; color: var(--text); }
.svc-detail__meta { font-size: var(--text-xs); color: var(--muted); font-family: var(--font-mono); }
.sw-empty { margin: var(--space-3) var(--space-4) 0; }
.svc-loading { margin: 0; padding: var(--space-3) var(--space-4) 0; font-size: var(--text-xs); color: var(--faint); }
</style>
