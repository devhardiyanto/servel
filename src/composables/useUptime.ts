import { ref, computed, onMounted, onUnmounted } from 'vue'

const uptimeSeconds = ref(0)
let interval: ReturnType<typeof setInterval> | null = null
let refCount = 0

export function useUptime() {
  onMounted(() => {
    refCount++
    if (!interval) {
      interval = setInterval(() => { uptimeSeconds.value++ }, 1000)
    }
  })

  onUnmounted(() => {
    refCount--
    if (refCount <= 0 && interval) {
      clearInterval(interval)
      interval = null
      refCount = 0
    }
  })

  const formatted = computed(() => {
    const s = uptimeSeconds.value
    const h = Math.floor(s / 3600)
    const m = Math.floor((s % 3600) / 60)
    const sec = s % 60
    const p = (n: number) => String(n).padStart(2, '0')
    return `${p(h)}:${p(m)}:${p(sec)}`
  })

  return { formatted }
}
