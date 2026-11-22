// Urutan dan kelompok project. Dipakai layar Projects, popup tray, dan palet
// perintah supaya ketiganya menampilkan urutan yang sama: kelompok sesuai
// urutan `groups`, project sesuai urutan `projects`, Ungrouped terakhir.
import type { Project, ProjectGroup } from '@/types/mise'

export interface ProjectSection {
  /** `null` = Ungrouped. */
  group: ProjectGroup | null
  projects: Project[]
}

/** Id kelompok project yang masih ada; `null` = Ungrouped. */
export function groupIdOf(p: Project, groups: ProjectGroup[]): string | null {
  return p.groupId && groups.some((g) => g.id === p.groupId) ? p.groupId : null
}

export function groupNameOf(p: Project, groups: ProjectGroup[]): string {
  const id = groupIdOf(p, groups)
  return id ? (groups.find((g) => g.id === id)?.name ?? '') : ''
}

/** Semua kelompok (termasuk yang kosong), lalu Ungrouped bila berisi atau bila tidak ada kelompok. */
export function projectSections(projects: Project[], groups: ProjectGroup[]): ProjectSection[] {
  const out: ProjectSection[] = groups.map((g) => ({ group: g, projects: projects.filter((p) => p.groupId === g.id) }))
  const ungrouped = projects.filter((p) => groupIdOf(p, groups) === null)
  if (ungrouped.length || groups.length === 0) out.push({ group: null, projects: ungrouped })
  return out
}

export function orderedProjects(projects: Project[], groups: ProjectGroup[]): Project[] {
  return projectSections(projects, groups).flatMap((s) => s.projects)
}

/**
 * Pindahkan project ke kelompok `groupId` (null = Ungrouped), tepat sebelum
 * `beforeId`, atau di akhir kelompok itu bila `beforeId` null. Mengembalikan
 * array baru; posisi di array hanya berarti relatif terhadap project lain
 * di kelompok yang sama.
 */
export function moveProject(
  projects: Project[],
  groups: ProjectGroup[],
  id: string,
  groupId: string | null,
  beforeId: string | null,
): Project[] {
  const moving = projects.find((p) => p.id === id)
  if (!moving || beforeId === id) return projects
  const rest = projects.filter((p) => p.id !== id)
  const moved: Project = { ...moving, groupId }
  let at = beforeId ? rest.findIndex((p) => p.id === beforeId) : -1
  if (at < 0) {
    const lastInGroup = rest.reduce((last, p, i) => (groupIdOf(p, groups) === groupId ? i : last), -1)
    at = lastInGroup >= 0 ? lastInGroup + 1 : rest.length
  }
  return [...rest.slice(0, at), moved, ...rest.slice(at)]
}

/** Geser project satu langkah di dalam kelompoknya. `null` bila sudah di ujung. */
export function shiftProject(projects: Project[], groups: ProjectGroup[], id: string, delta: -1 | 1): Project[] | null {
  const p = projects.find((x) => x.id === id)
  if (!p) return null
  const gid = groupIdOf(p, groups)
  const same = projects.filter((x) => groupIdOf(x, groups) === gid)
  const i = same.findIndex((x) => x.id === id)
  const j = i + delta
  if (j < 0 || j >= same.length) return null
  const before = delta < 0 ? same[j].id : (same[j + 1]?.id ?? null)
  return moveProject(projects, groups, id, gid, before)
}
