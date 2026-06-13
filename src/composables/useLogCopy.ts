import type { Ref } from 'vue'
import type { LogLine } from './useLogs'

/**
 * Attach copy + Ctrl+A scoping to a log container element ref.
 * - Ctrl+A inside container → select only container contents (not whole window).
 * - Copy event → reformat clipboard sebagai plain text aligned `ts  [SRC]  text`.
 */
export function useLogCopy(
  containerRef: Ref<HTMLElement | null>,
  linesRef: Ref<LogLine[]>,
) {
  function formatLine(l: LogLine): string {
    return `${l.ts}  [${l.src}]  ${l.text}`
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'a') return
    const target = e.target as Node | null
    if (!target || !containerRef.value?.contains(target)) return
    e.preventDefault()
    const range = document.createRange()
    range.selectNodeContents(containerRef.value)
    const sel = window.getSelection()
    sel?.removeAllRanges()
    sel?.addRange(range)
  }

  function handleCopy(e: ClipboardEvent): void {
    const sel = window.getSelection()
    if (!sel || sel.isCollapsed) return
    const anchor = sel.anchorNode
    if (!anchor || !containerRef.value?.contains(anchor)) return

    const container = containerRef.value
    const allLineEls = Array.from(container.querySelectorAll<HTMLElement>('[data-log-idx]'))
    const picked: number[] = []
    for (const el of allLineEls) {
      try {
        if (sel.containsNode(el, true)) {
          const idx = Number(el.dataset.logIdx)
          if (!Number.isNaN(idx)) picked.push(idx)
        }
      } catch {
        // ignore
      }
    }
    if (picked.length === 0) return

    const text = picked
      .map((i) => linesRef.value[i])
      .filter((l): l is LogLine => !!l)
      .map(formatLine)
      .join('\n')

    e.preventDefault()
    e.clipboardData?.setData('text/plain', text)
  }

  function focusContainer(): void {
    containerRef.value?.focus()
  }

  return { handleKeydown, handleCopy, focusContainer }
}
