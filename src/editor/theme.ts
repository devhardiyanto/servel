// Tampilan CodeMirror dari token Kova (tema gelap tetap, lihat canvas E1).
import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'

/** Font kode: Cascadia Mono (Windows 11) atau Consolas, tanpa ligatur. */
export const CODE_FONT = "'Cascadia Mono', Consolas, 'DejaVu Sans Mono', monospace"

export const kovaTheme = EditorView.theme(
  {
    '&': { height: '100%', color: 'var(--text)', backgroundColor: 'var(--bg)', fontSize: '13px' },
    '.cm-scroller': { fontFamily: CODE_FONT, lineHeight: '20px', fontVariantLigatures: 'none' },
    '.cm-content': { caretColor: 'var(--text)', padding: '4px 0' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--text)' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: 'rgba(111,195,234,.25)' },
    '.cm-activeLine': { backgroundColor: 'rgba(255,255,255,.035)' },
    // Nomor baris memakai --faint: kontras cukup di atas --bg (audit-005 U25).
    '.cm-gutters': { backgroundColor: 'var(--bg)', color: 'var(--faint)', border: 'none' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text)' },
    '.cm-lineNumbers .cm-gutterElement': { padding: '0 16px 0 12px', minWidth: '40px' },
    '.cm-matchingBracket': { backgroundColor: 'rgba(111,195,234,.18)', outline: '1px solid rgba(111,195,234,.4)' },
    '.cm-searchMatch': { backgroundColor: 'rgba(240,198,116,.25)' },
    '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'rgba(240,198,116,.45)' },
    '.cm-panels': { backgroundColor: 'var(--surface)', color: 'var(--text)' },
    '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--border)' },
    '.cm-panel input, .cm-panel button': { fontFamily: 'var(--font-sans)', fontSize: '12px' },
    '.cm-tooltip': { backgroundColor: 'var(--surface2)', border: '1px solid var(--border-strong)', borderRadius: '6px' },
    '.cm-tooltip-autocomplete ul li[aria-selected]': { backgroundColor: 'var(--surface3)', color: 'var(--text)' },
    '.cm-foldPlaceholder': { backgroundColor: 'var(--surface2)', border: 'none', color: 'var(--muted)' },
  },
  { dark: true },
)

export const kovaHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword], color: '#c3a6f0' },
    { tag: [t.string, t.special(t.string), t.regexp], color: '#a9dcae' },
    { tag: [t.function(t.variableName), t.function(t.propertyName)], color: '#e8c98a' },
    { tag: [t.variableName, t.self], color: '#f0b9a2' },
    { tag: [t.typeName, t.className, t.namespace, t.tagName], color: '#8ad0f0' },
    { tag: [t.number, t.bool, t.null, t.atom], color: '#f0c674' },
    { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: 'var(--faint)', fontStyle: 'italic' },
    { tag: [t.attributeName, t.propertyName], color: '#b8c7e0' },
    { tag: [t.heading], color: '#8ad0f0', fontWeight: '600' },
    { tag: [t.link, t.url], color: '#6fc3ea', textDecoration: 'underline' },
    { tag: t.invalid, color: 'var(--red)' },
  ]),
)
