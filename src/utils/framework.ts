// Cocokkan jenis project dengan logo framework. `kind` di config berisi label
// deteksi ("Vite · Vue", "Next.js") atau id lama ("laravel"), jadi keduanya
// dikenali. Tanpa kecocokan = chip abu-abu biasa.
import { FRAMEWORK_ICONS, type FrameworkIcon } from '@/utils/frameworkIcons'

const ALIASES: Record<string, string> = {
  'next.js': 'next',
  nextjs: 'next',
  'node.js': 'node',
  nodejs: 'node',
  'vue.js': 'vue',
  adonisjs: 'adonis',
  nestjs: 'nest',
}

export function frameworkKey(kind?: string | null): string | null {
  const k = (kind ?? '').trim().toLowerCase()
  if (!k) return null
  // "Vite · React" memakai logo library-nya, bukan Vite.
  for (const lib of ['react', 'vue', 'svelte']) if (k.includes(lib)) return lib
  const head = k.split(/[\s·]+/)[0]
  const key = ALIASES[head] ?? head
  return key in FRAMEWORK_ICONS ? key : null
}

export function frameworkIcon(kind?: string | null): FrameworkIcon | null {
  const key = frameworkKey(kind)
  return key ? FRAMEWORK_ICONS[key] : null
}

/** Nilai CSS warna merek; merek hitam memakai warna teks tema. */
export function frameworkColor(icon: FrameworkIcon): string {
  return icon.color ?? 'var(--text)'
}
