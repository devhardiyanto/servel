import { createApp } from 'vue'
import EditorApp from './EditorApp.vue'
import { installEditorGuards } from './keys'

export function mountEditor(sel: string): void {
  if (import.meta.env.PROD) installEditorGuards()
  document.documentElement.classList.add('kova-editor')
  createApp(EditorApp).mount(sel)
}
