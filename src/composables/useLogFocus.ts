// Permintaan "buka Logs untuk project ini" dari layar lain (v3.2). Layar Logs
// membacanya sekali saat dibuka.
import { ref } from 'vue'

const pendingProject = ref<string | null>(null)

export function useLogFocus() {
  function focusProject(name: string): void {
    pendingProject.value = name
  }
  function take(): string | null {
    const name = pendingProject.value
    pendingProject.value = null
    return name
  }
  return { focusProject, take }
}
