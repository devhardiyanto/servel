<script setup lang="ts">
// Runtimes › Add tool (Flow L5): tab baru untuk tool mise lain. Versi terbaru
// langsung dipasang; versi pertama jadi global (seperti tool bawaan).
import { computed, ref, watch } from 'vue'
import UiModal from '@/components/ui/UiModal.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiSegmented from '@/components/ui/UiSegmented.vue'
import { useConfig } from '@/composables/useConfig'
import { useRuntimeInstall, toolLabel } from '@/composables/useRuntimeInstall'

const props = defineProps<{ open: boolean; distros: string[] }>()
const emit = defineEmits<{ close: []; added: [tool: string] }>()

const { config, addExtraTool } = useConfig()
const rt = useRuntimeInstall()

const POPULAR: { tool: string; desc: string }[] = [
  { tool: 'bun', desc: 'JavaScript runtime and package manager' },
  { tool: 'deno', desc: 'JavaScript / TypeScript runtime' },
  { tool: 'rust', desc: 'rustc + cargo' },
  { tool: 'java', desc: 'OpenJDK' },
  { tool: 'ruby', desc: 'ruby + gem' },
  { tool: 'dotnet', desc: '.NET SDK' },
  { tool: 'zig', desc: 'Zig compiler' },
  { tool: 'elixir', desc: 'Elixir (needs erlang)' },
  { tool: 'erlang', desc: 'Erlang/OTP' },
  { tool: 'terraform', desc: 'HashiCorp Terraform' },
  { tool: 'kubectl', desc: 'Kubernetes CLI' },
  { tool: 'pnpm', desc: 'Node package manager' },
]

const query = ref('')
const pick = ref('')
const where = ref('')
const error = ref('')
const working = ref(false)

watch(() => props.open, (o) => {
  if (!o) return
  query.value = ''
  pick.value = ''
  where.value = ''
  error.value = ''
})

const have = computed(() => new Set(['php', 'node', 'go', 'python', ...(config.value.extraTools ?? [])]))
const shown = computed(() => {
  const q = query.value.trim().toLowerCase()
  return POPULAR.filter((t) => !have.value.has(t.tool) && (!q || t.tool.includes(q) || t.desc.toLowerCase().includes(q)))
})
/** Nama yang diketik dan bukan dari daftar: tool mise apa pun. */
const typed = computed(() => {
  const q = query.value.trim().toLowerCase()
  return q && /^[a-z0-9][a-z0-9._:/-]{0,63}$/.test(q) && !POPULAR.some((t) => t.tool === q) && !have.value.has(q) ? q : ''
})
const chosen = computed(() => pick.value || typed.value)
const whereOptions = computed(() => [{ value: '', label: 'Windows' }, ...props.distros.map((d) => ({ value: d, label: `WSL · ${d}` }))])

async function add(): Promise<void> {
  const t = chosen.value
  if (!t) return
  working.value = true
  error.value = ''
  try {
    await addExtraTool(t)
    emit('added', t)
    emit('close')
    const r = await rt.install(t, 'latest', where.value || null)
    if (r && !r.ok && !r.cancelled) error.value = r.error
  } catch (err) {
    error.value = String(err)
  } finally {
    working.value = false
  }
}
</script>

<template>
  <UiModal :open="open" title="Add tool" @close="emit('close')">
    <p class="atm-text">Any tool mise can install. It gets its own tab next to PHP, Node, Go and Python.</p>
    <UiInput v-model="query" aria-label="Search mise tools" placeholder="Search, or type a mise tool name" data-autofocus />
    <div class="atm-list" role="radiogroup" aria-label="Tool">
      <label v-for="t in shown" :key="t.tool" class="atm-row" :class="{ 'atm-row--on': pick === t.tool }">
        <input v-model="pick" type="radio" name="atm-tool" :value="t.tool" />
        <span class="atm-name">{{ toolLabel(t.tool) }}</span>
        <span class="atm-desc">{{ t.desc }}</span>
        <span class="atm-mono">{{ t.tool }}</span>
      </label>
      <p v-if="typed" class="atm-text">Press Add to use <code class="atm-mono">{{ typed }}</code> from the mise registry.</p>
      <p v-else-if="shown.length === 0" class="atm-text">No match. Type the exact mise tool name, e.g. <code class="atm-mono">aqua:cli/cli</code>.</p>
    </div>
    <div v-if="distros.length" class="atm-where">
      <span class="atm-label">Install on</span>
      <UiSegmented v-model="where" :options="whereOptions" label="Install on" />
    </div>
    <p v-if="error" class="atm-err" role="alert">{{ error }}</p>
    <template #footer>
      <UiButton size="md" @click="emit('close')">Cancel</UiButton>
      <UiButton variant="primary" size="md" :disabled="!chosen" :loading="working" @click="add">{{ chosen ? `Add ${toolLabel(chosen)} and install latest` : 'Add' }}</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.atm-text { margin: 0; color: var(--muted); font-size: var(--text-sm); }
.atm-list { display: flex; flex-direction: column; gap: var(--space-1); max-height: 300px; overflow: auto; }
.atm-row { display: flex; align-items: center; gap: var(--space-3); min-height: 40px; padding: 0 var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); cursor: pointer; font-size: var(--text-sm); }
.atm-row--on { border-color: var(--accent); background: var(--surface2); }
.atm-name { font-weight: 600; width: 84px; }
.atm-desc { flex: 1; color: var(--muted); }
.atm-mono { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--faint); }
.atm-where { display: flex; flex-direction: column; gap: var(--space-2); }
.atm-label { font-size: var(--text-xs); font-weight: 500; color: var(--muted); }
.atm-err { margin: 0; color: var(--red); font-size: var(--text-sm); }
</style>
