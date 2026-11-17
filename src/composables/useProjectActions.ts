// Aksi project yang dipakai daftar, panel detail, palet, dan dialog (v3.4).
// State dibagi (singleton) supaya error dan status sibuk satu project terlihat
// sama di daftar dan di panel detail.
import { inject, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useConfig } from '@/composables/useConfig'
import { useProjectRuntime, type SiteState } from '@/composables/useProjectRuntime'
import { useLogFocus } from '@/composables/useLogFocus'
import { useToast } from '@/composables/useToast'
import { SetViewKey } from '@/types/navigation'
import type { Project } from '@/types/mise'
import type { MenuItem } from '@/components/ui/UiMenu.vue'

const errors = ref<Record<string, string>>({})
const busy = ref<Record<string, boolean>>({})
/** Project yang drawer "Project settings"-nya terbuka. */
const editingId = ref<string | null>(null)
/** Project yang sedang dikonfirmasi untuk dihapus. */
const removingId = ref<string | null>(null)
/** Modal Add project. */
const adding = ref(false)
/** Tab panel detail; diubah dari luar ("Environment" di banner). */
const detailTab = ref<'overview' | 'env' | 'logs'>('overview')

const STATE_LABEL: Record<SiteState, string> = {
  stopped: 'stopped',
  starting: 'starting',
  running: 'running',
  unresponsive: 'not answering',
  exited: 'stopped',
}

export function projectUrl(p: Project): string {
  return `http://localhost:${p.port}`
}

export function canRun(p: Project): boolean {
  return !!p.path && !!p.command?.trim() && (p.port ?? 0) >= 1024
}

export function missingReason(p: Project): string {
  if (!p.path) return 'Choose the project folder first.'
  if (!p.command?.trim()) return 'No command yet. Open Project settings and fill it in.'
  if ((p.port ?? 0) < 1024) return 'Set a port (1024–65535) in Project settings.'
  return ''
}

export function useProjectActions() {
  const setView = inject(SetViewKey, () => {})
  const { config } = useConfig()
  const runtime = useProjectRuntime()
  const logFocus = useLogFocus()
  const toast = useToast()

  function setError(id: string, msg: string): void {
    errors.value = { ...errors.value, [id]: msg }
  }

  async function run(p: Project, fn: () => Promise<void>): Promise<void> {
    setError(p.id, '')
    busy.value = { ...busy.value, [p.id]: true }
    try {
      await fn()
    } catch (err) {
      setError(p.id, String(err))
    } finally {
      busy.value = { ...busy.value, [p.id]: false }
    }
  }

  function stateLabel(p: Project): string {
    const info = runtime.infoOf(p.id)
    if (info?.state === 'exited') return info.exitCode !== null ? `exited (${info.exitCode})` : 'exited'
    return STATE_LABEL[runtime.stateOf(p.id)]
  }

  function takenPorts(exceptId?: string): number[] {
    return config.value.projects.filter((p) => p.id !== exceptId && (p.port ?? 0) > 0).map((p) => p.port as number)
  }

  async function open(p: Project): Promise<void> {
    try {
      await openUrl(projectUrl(p))
    } catch {
      window.open(projectUrl(p), '_blank', 'noopener')
    }
  }

  function viewLog(p: Project): void {
    logFocus.focusProject(p.name)
    setView('logs')
  }

  function menuItems(p: Project): MenuItem[] {
    const hasFolder = !!p.path
    const items: MenuItem[] = [
      { id: 'folder', label: 'Open folder', disabled: !hasFolder },
      { id: 'terminal', label: 'Terminal here', disabled: !hasFolder },
      { id: 'vscode', label: 'Open in VS Code', disabled: !hasFolder },
    ]
    if ((p.port ?? 0) > 0) items.push({ id: 'copy-url', label: 'Copy URL', hint: `localhost:${p.port}` })
    items.push({ id: 'settings', label: 'Project settings…', separator: true })
    items.push({ id: 'remove', label: 'Remove project…', separator: true, danger: true })
    return items
  }

  async function onMenu(p: Project, id: string): Promise<void> {
    setError(p.id, '')
    try {
      if (id === 'folder' || id === 'terminal' || id === 'vscode') {
        await invoke('project_open_in', { target: id, path: p.path })
      } else if (id === 'copy-url') {
        await navigator.clipboard.writeText(projectUrl(p))
        toast.show(`Copied ${projectUrl(p)}`)
      } else if (id === 'settings') {
        editingId.value = p.id
      } else if (id === 'remove') {
        removingId.value = p.id
      }
    } catch (err) {
      setError(p.id, String(err))
    }
  }

  return {
    errors,
    busy,
    editingId,
    removingId,
    adding,
    detailTab,
    stateLabel,
    takenPorts,
    setError,
    start: (p: Project) => run(p, () => runtime.start(p)),
    stop: (p: Project) => run(p, () => runtime.stop(p.id)),
    restart: (p: Project) => run(p, () => runtime.restart(p)),
    open,
    viewLog,
    menuItems,
    onMenu,
  }
}
