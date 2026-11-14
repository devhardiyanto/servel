<script setup lang="ts">
import { computed, ref } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import UiButton from '@/components/ui/UiButton.vue'
import type { ServiceDef } from '@/types/service'
import { connectionInfo } from '@/utils/connect'

const props = defineProps<{ def: ServiceDef; running: boolean }>()

const info = computed(() => connectionInfo(props.def))
const copied = ref<'' | 'env' | 'url'>('')
let timer: ReturnType<typeof setTimeout> | undefined

async function copy(kind: 'env' | 'url'): Promise<void> {
  const text = kind === 'env' ? info.value.env : info.value.url
  try {
    await navigator.clipboard.writeText(text)
    copied.value = kind
    clearTimeout(timer)
    timer = setTimeout(() => (copied.value = ''), 1500)
  } catch {
    copied.value = ''
  }
}
</script>

<template>
  <div class="sc" :aria-label="`Connect to ${def.name}`">
    <dl class="sc-fields">
      <template v-for="f in info.fields" :key="f.label">
        <dt>{{ f.label }}</dt>
        <dd>{{ f.value }}</dd>
      </template>
    </dl>
    <code class="sc-url">{{ info.url }}</code>
    <div class="sc-actions">
      <span v-if="!running" class="sc-note">Not running. Start it to connect.</span>
      <UiButton v-if="info.webUrl" variant="quiet" @click="openUrl(info.webUrl)">Open web UI</UiButton>
      <UiButton variant="quiet" @click="copy('url')">{{ copied === 'url' ? 'Copied' : 'Copy URL' }}</UiButton>
      <UiButton v-if="info.env" variant="accent" @click="copy('env')">{{ copied === 'env' ? 'Copied' : 'Copy .env' }}</UiButton>
    </div>
  </div>
</template>

<style scoped>
.sc {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: var(--space-2) var(--space-4);
  align-items: center;
  margin: 0 var(--space-4) var(--space-2) 76px;
  padding: var(--space-3) var(--space-4);
  background: var(--bg-sunken);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}
.sc-fields {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 2px var(--space-4);
  margin: 0;
  font-size: var(--text-xs);
}
.sc-fields dt { color: var(--faint); }
.sc-fields dd { margin: 0; font-family: var(--font-mono); color: var(--text); user-select: text; }
.sc-url {
  grid-column: 1 / -1;
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--muted);
  user-select: text;
  overflow-wrap: anywhere;
}
.sc-actions {
  grid-column: 1 / -1;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-2);
}
.sc-note { margin-right: auto; font-size: var(--text-xs); color: var(--amber); }
</style>
