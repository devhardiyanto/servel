import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  MISE_NEEDS_SCOPE,
  MISE_UNTRUSTED,
  type MiseContext,
  type ScopeRequest,
} from '@/types/mise'

// Singleton: konteks versi (Global / project) dipakai Dashboard, kartu PHP/Node,
// dan Settings sekaligus.
const ctx = ref<MiseContext | null>(null)
const scopeRequest = ref<ScopeRequest | null>(null)
const untrustedPath = ref<string | null>(null)
const lastError = ref<string | null>(null)
const busy = ref(false)

function parseUntrusted(err: string | null | undefined): string | null {
  if (!err || !err.startsWith(MISE_UNTRUSTED + '|')) return null
  return err.slice(MISE_UNTRUSTED.length + 1)
}

async function refresh(): Promise<void> {
  try {
    ctx.value = await invoke<MiseContext>('mise_context')
    untrustedPath.value = parseUntrusted(ctx.value.error)
    if (ctx.value.error && !untrustedPath.value) lastError.value = ctx.value.error
  } catch (err) {
    lastError.value = String(err)
  }
}

/**
 * Tangani error dari php_switch / node_switch.
 * Return `true` bila error sudah ditangani di UI (dialog scope / banner trust),
 * sehingga caller tidak perlu menampilkannya lagi.
 */
function handleSwitchError(tool: 'php' | 'node', version: string, err: unknown): boolean {
  const msg = typeof err === 'string' ? err : String(err)
  if (msg.startsWith(MISE_NEEDS_SCOPE + '|')) {
    const [, , projectFile] = msg.split('|')
    scopeRequest.value = {
      tool,
      version,
      projectFile: projectFile ?? '',
      globalVersion: ctx.value?.[tool]?.version ?? null,
    }
    return true
  }
  const untrusted = parseUntrusted(msg)
  if (untrusted) {
    untrustedPath.value = untrusted
    return true
  }
  lastError.value = msg
  return false
}

async function trust(path: string): Promise<void> {
  busy.value = true
  try {
    await invoke<void>('mise_trust', { path })
    untrustedPath.value = null
    await refresh()
  } catch (err) {
    lastError.value = String(err)
  } finally {
    busy.value = false
  }
}

async function repair(action: 'reshim' | 'cache-clear' | 'doctor'): Promise<void> {
  busy.value = true
  try {
    await invoke<void>('mise_repair', { action })
  } catch (err) {
    lastError.value = String(err)
  } finally {
    busy.value = false
  }
}

const isMise = computed(() => ctx.value?.provider === 'mise')

/** Label sumber versi untuk ditampilkan di kartu. */
function sourceLabel(tool: 'php' | 'node'): string | null {
  const t = ctx.value?.[tool]
  if (!isMise.value || !t) return null
  if (t.scope === 'global') return 'global'
  if (t.scope === 'none') return 'not pinned'
  return t.source ?? 'project'
}

export function useVersionContext() {
  return {
    ctx,
    isMise,
    scopeRequest,
    untrustedPath,
    lastError,
    busy,
    refresh,
    handleSwitchError,
    trust,
    repair,
    sourceLabel,
  }
}
