// Aksi project yang dipakai daftar, panel detail, palet, dan dialog.
// State dibagi (singleton) supaya error dan status sibuk satu project terlihat
// sama di daftar dan di panel detail.
import { inject, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useConfig } from '@/composables/useConfig'
import { isActive, useProjectRuntime, type SiteState } from '@/composables/useProjectRuntime'
import { useLogFocus } from '@/composables/useLogFocus'
import { useToast } from '@/composables/useToast'
import { announce } from '@/composables/useStatusAnnouncer'
import { SetViewKey } from '@/types/navigation'
import type { Project, ProjectGroup } from '@/types/mise'
import type { MenuItem } from '@/components/ui/UiMenu.vue'
import { groupIdOf, moveProject, shiftProject } from '@/utils/projectGroups'

const errors = ref<Record<string, string>>({})
const busy = ref<Record<string, boolean>>({})
/** Project yang drawer "Project settings"-nya terbuka. */
const editingId = ref<string | null>(null)
/** Project yang sedang dikonfirmasi untuk dihapus. */
const removingId = ref<string | null>(null)
/** Modal Add project. */
const adding = ref(false)
/** Project yang harus dipilih saat layar Projects dibuka (dari Logs). */
const selectRequest = ref<string | null>(null)
/** Tab panel detail; diubah dari luar ("Environment" di banner). */
const detailTab = ref<'overview' | 'env' | 'logs'>('overview')
/**
 * Kelompok baru yang namanya sedang diketik (belum tersimpan). `projectId`
 * = project yang ikut dipindah setelah nama disimpan ("Move to group › New group…").
 */
const draftGroup = ref<{ projectId: string | null } | null>(null)
/** Kelompok yang namanya sedang diubah di tempat. */
const renamingGroupId = ref<string | null>(null)

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

/** Project dijalankan sebagai container (Dockerfile / compose), bukan command. */
export function isContainerMode(p: Project): boolean {
  return p.runMode === 'dockerfile' || p.runMode === 'compose'
}

export function canRun(p: Project): boolean {
  return !!p.path && (isContainerMode(p) || !!p.command?.trim()) && (p.port ?? 0) >= 1024
}

export function missingReason(p: Project): string {
  if (!p.path) return 'Choose the project folder first.'
  if (!isContainerMode(p) && !p.command?.trim()) return 'No command yet. Open Project settings and fill it in.'
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

  const groups = (): ProjectGroup[] => config.value.groups ?? []

  /** Kalimat pengumuman posisi project di kelompoknya. */
  function positionText(id: string): string {
    const p = config.value.projects.find((x) => x.id === id)
    if (!p) return ''
    const gid = groupIdOf(p, groups())
    const same = config.value.projects.filter((x) => groupIdOf(x, groups()) === gid)
    const where = gid ? (groups().find((g) => g.id === gid)?.name ?? '') : 'Ungrouped'
    return `${p.name} moved to position ${same.findIndex((x) => x.id === id) + 1} of ${same.length} in ${where}`
  }

  /** Geser satu langkah di kelompoknya (Alt+↑/↓, Move up/down). */
  async function shift(p: Project, delta: -1 | 1): Promise<boolean> {
    const next = shiftProject(config.value.projects, groups(), p.id, delta)
    if (!next) return false
    await useConfig().setProjects(next)
    announce(positionText(p.id))
    return true
  }

  /** Pindah ke kelompok lain (null = Ungrouped), sebelum `beforeId` atau di akhir. */
  async function moveTo(p: Project, groupId: string | null, beforeId: string | null = null): Promise<void> {
    await useConfig().setProjects(moveProject(config.value.projects, groups(), p.id, groupId, beforeId))
    announce(positionText(p.id))
  }

  function canShift(p: Project, delta: -1 | 1): boolean {
    return shiftProject(config.value.projects, groups(), p.id, delta) !== null
  }

  /** Start semua project di kelompok yang bisa dijalankan dan belum aktif. */
  function startGroup(gid: string | null): void {
    for (const p of config.value.projects) {
      if (groupIdOf(p, groups()) === gid && canRun(p) && !isActive(runtime.stateOf(p.id))) void run(p, () => runtime.start(p))
    }
  }

  function stopGroup(gid: string | null): void {
    for (const p of config.value.projects) {
      if (groupIdOf(p, groups()) === gid && isActive(runtime.stateOf(p.id))) void run(p, () => runtime.stop(p.id))
    }
  }

  function menuItems(p: Project): MenuItem[] {
    const hasFolder = !!p.path
    const items: MenuItem[] = [
      { id: 'folder', label: 'Open folder', disabled: !hasFolder },
      { id: 'terminal', label: 'Terminal here', disabled: !hasFolder },
      { id: 'vscode', label: 'Open in VS Code', disabled: !hasFolder },
    ]
    if ((p.port ?? 0) > 0) items.push({ id: 'copy-url', label: 'Copy URL', hint: `localhost:${p.port}` })
    const current = groupIdOf(p, groups())
    items.push({
      id: 'move',
      label: 'Move to group',
      separator: true,
      children: [
        ...groups().map((g) => ({ id: `move:${g.id}`, label: g.name, disabled: g.id === current, hint: g.id === current ? 'current' : undefined })),
        { id: 'move:', label: 'Ungrouped', disabled: current === null, hint: current === null ? 'current' : undefined },
        { id: 'move-new', label: 'New group…', separator: true },
      ],
    })
    items.push({ id: 'up', label: 'Move up', hint: 'Alt+↑', disabled: !canShift(p, -1) })
    items.push({ id: 'down', label: 'Move down', hint: 'Alt+↓', disabled: !canShift(p, 1) })
    items.push({ id: 'settings', label: 'Project settings…', separator: true })
    items.push({ id: 'remove', label: 'Remove project…', separator: true, danger: true })
    return items
  }

  async function onMenu(p: Project, id: string): Promise<void> {
    setError(p.id, '')
    try {
      if (id === 'folder' || id === 'terminal' || id === 'vscode') {
        await invoke('project_open_in', { target: id, path: p.path, projectId: p.id })
      } else if (id === 'copy-url') {
        await navigator.clipboard.writeText(projectUrl(p))
        toast.show(`Copied ${projectUrl(p)}`)
      } else if (id === 'settings') {
        editingId.value = p.id
      } else if (id === 'remove') {
        removingId.value = p.id
      } else if (id === 'up' || id === 'down') {
        await shift(p, id === 'up' ? -1 : 1)
      } else if (id === 'move-new') {
        renamingGroupId.value = null
        draftGroup.value = { projectId: p.id }
      } else if (id.startsWith('move:')) {
        await moveTo(p, id.slice(5) || null)
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
    selectRequest,
    draftGroup,
    renamingGroupId,
    shift,
    moveTo,
    startGroup,
    stopGroup,
    /** Buka layar Projects dengan project ini terpilih. */
    openProject(p: Project): void {
      selectRequest.value = p.id
      setView('projects')
    },
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
