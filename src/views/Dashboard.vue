<script setup lang="ts">
import UiButton from '@/components/ui/UiButton.vue'
import UiInput from '@/components/ui/UiInput.vue'
import { onMounted, onUnmounted, computed, inject, ref } from 'vue'
import { useLogs } from '@/composables/useLogs'
import { useServices } from '@/composables/useServices'
import { useConfig } from '@/composables/useConfig'
import { useEngineStatus } from '@/composables/useEngineStatus'
import { SetViewKey } from '@/types/navigation'
import LogTail from '@/components/LogTail.vue'
import ServiceRow from '@/components/ServiceRow.vue'
import ServiceDetail from '@/components/ServiceDetail.vue'
import SkeletonBox from '@/components/ui/SkeletonBox.vue'
import UiBanner from '@/components/ui/UiBanner.vue'

const SKELETON_ROW_COUNT = 9

const setView = inject(SetViewKey)!

// Panel Connect (v2.10): satu service terbuka sekaligus.
const connectId = ref<string | null>(null)
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
  runningCount,
  selectedCount,
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

function onProfileChange(id: string): void {
  if (id) applyProfile(id)
}

onMounted(async () => {
  await loadServices()
  await useConfig().load()

  const cfg = useConfig().config.value

  // Restore selection UI dari config
  if (cfg.selectedServiceIds.length > 0) {
    setSelectedIds(cfg.selectedServiceIds)
  }

  // Rekonsiliasi: container yang auto-restart (restart: unless-stopped) setelah
  // reboot tampil running → switch harus ON walau tidak ada di saved config.
  // Dipanggil SETELAH config loaded + setSelectedIds; persist watch akan menulis
  // ulang selection dari running state (menyembuhkan config yang hilang).
  reconcileSelectedWithRunning()

})

onUnmounted(() => {
})
</script>

<template>
  <div class="view view-dashboard">
    <div class="dashboard-content">
    <UiBanner v-if="engineStatus === 'down'" tone="err" :title="`${engineLabel} is not responding`">
      <template v-if="engine === 'wslc'">Run <code>wsl --update</code>, then Retry.</template>
      <template v-else-if="canSwitchToWslc">Start Docker Desktop, or switch to the built-in WSL containers.</template>
      <template v-else>Start Docker Desktop to use services.</template>
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

    <UiBanner v-if="portConflicts.length > 0" tone="warn" title="A port is used by another program">
      <template v-for="(c, i) in portConflicts" :key="c.serviceId + ':' + c.port">
        {{ c.serviceName }} ({{ c.port }})<span v-if="i < portConflicts.length - 1">, </span>
      </template>
      <template #actions>
        <UiButton @click="startIgnoringConflicts">Start anyway</UiButton>
        <UiButton variant="quiet" @click="dismissPortConflicts">Close</UiButton>
      </template>
    </UiBanner>

    <UiBanner v-if="serviceError" tone="err" title="A service did not start">{{ serviceError }}</UiBanner>

    <section class="services-wrap">
      <header class="sw-header">
        <span class="sw-title">Services</span>
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
          <UiButton
            variant="danger"
            :disabled="hasStopping || runningCount === 0"
            @click="stopAll"
          >
            &#9632; Stop all
          </UiButton>
          <UiButton
            variant="success"
            :disabled="hasStarting"
            @click="start()"
          >
            &#9654; Start selected
          </UiButton>
        </div>
      </header>
      <UiBanner
        v-if="definitions.length > 0 && selectedCount === 0 && engineStatus !== 'down'"
        class="sw-empty"
        tone="info"
        :title="`No services in ${activeProfileName} yet`"
      >
        Turn on the services this profile starts with the switches below. Projects that use {{ activeProfileName }} can start them together with the project.
      </UiBanner>
      <div class="services-table">
        <template v-if="coreServices.length > 0">
          <div class="svc-group-label">core</div>
          <template v-for="def in coreServices" :key="def.id">
            <ServiceRow
              :def="def"
              :state="uiState[def.id]"
              :connect-open="connectId === def.id"
              :unknown="engineStatus === 'down'"
              @toggle="toggle"
              @connect="toggleConnect"
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
              :unknown="engineStatus === 'down'"
              @toggle="toggle"
              @connect="toggleConnect"
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
    </section>

    <section class="log-wrap">
      <div class="log-wrap__bar">
        <span class="log-wrap__label">LOGS</span>
        <UiButton variant="quiet" @click="setView('logs')">Open logs</UiButton>
      </div>
      <LogTail :lines="logLines" max-height="148px" />
    </section>
    </div>

    <aside v-if="connectDef" class="svc-detail" :aria-label="`${connectDef.name} details`">
      <ServiceDetail :key="connectDef.id" :def="connectDef" @close="connectId = null" />
    </aside>
  </div>
</template>

<style scoped>
.view-dashboard {
  display: flex;
  flex-direction: row;
  height: 100%;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
}

.app-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--space-6);
  height: 34px;
  border-bottom: 1px solid var(--border);
  background: var(--bg);
  flex-shrink: 0;
}

.app-strip__brand {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
}

.app-strip__name {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  font-weight: 700;
  color: var(--accent);
  letter-spacing: 0.02em;
}

.app-strip__tag {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--faint);
  letter-spacing: 0.04em;
}

.app-strip__right {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.app-strip__uptime {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--faint);
}

.app-strip__settings-btn {
  background: transparent;
  border: none;
  color: var(--faint);
  font-size: var(--text-sm);
  cursor: pointer;
  padding: 2px 4px;
  border-radius: var(--radius-xs);
  line-height: 1;
  transition: color 0.1s, background 0.1s;
}

.app-strip__settings-btn:hover {
  color: var(--text);
  background: var(--surface2);
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

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

.services-wrap {
  margin: 0;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.sw-header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-5);
  background: transparent;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.sw-title {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
}

.sw-counter {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  color: var(--faint);
  margin-right: auto;
}

.sw-profile {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin-right: var(--space-2);
}

.sw-profile__label {
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--faint);
}

.sw-actions {
  display: flex;
  gap: var(--space-2);
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

/* Skeleton row — mirror .svc-row grid sehingga real content tidak shift */
.svc-row-skeleton {
  display: grid;
  grid-template-columns: 36px 24px 1fr 100px 68px 80px 76px;
  align-items: center;
  gap: var(--space-3);
  padding: 5px var(--space-4);
  /* Height match-able dengan ServiceRow (svc-row punya font-size 12px + padding 5px) */
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
  width: 300px;
  flex-shrink: 0;
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
