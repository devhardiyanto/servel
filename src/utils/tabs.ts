/** Atribut panel untuk UiTabs: satu panel yang isinya berganti per tab. */
export function tabPanelAttrs(idBase: string, value: string) {
  return { id: `${idBase}-panel`, role: 'tabpanel', 'aria-labelledby': `${idBase}-tab-${value}`, tabindex: -1 }
}
