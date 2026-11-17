<script setup lang="ts">
// Pemisah antara daftar dan panel detail di kanan (v3.7.0).
// - Seret: kursor col-resize; lebar panel dijepit ke [min, max].
// - Seret melewati batas kiri panel (min - COLLAPSE_SLACK) lalu lepas: panel
//   ditutup (bila `collapsible`); pemanggil menampilkan tombol untuk membuka.
// - Keyboard: panah kiri/kanan ±16 px, Home = lebar maksimum, End = minimum,
//   Enter = tutup panel. Klik dua kali = lebar bawaan.
import { ref } from 'vue'

const props = withDefaults(
  defineProps<{
    width: number
    min: number
    max: number
    defaultWidth: number
    /** id panel yang diatur, untuk aria-controls. */
    controls: string
    label: string
    collapsible?: boolean
  }>(),
  { collapsible: true },
)
const emit = defineEmits<{ 'update:width': [value: number]; collapse: [] }>()

const COLLAPSE_SLACK = 80
const STEP = 16

const dragging = ref(false)
const willCollapse = ref(false)
let startX = 0
let startWidth = 0

function clamp(w: number): number {
  return Math.min(props.max, Math.max(props.min, Math.round(w)))
}

function onPointerDown(e: PointerEvent): void {
  if (e.button !== 0) return
  e.preventDefault()
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  dragging.value = true
  startX = e.clientX
  startWidth = props.width
}

function onPointerMove(e: PointerEvent): void {
  if (!dragging.value) return
  // Panel ada di kanan: seret ke kiri = panel melebar.
  const raw = startWidth - (e.clientX - startX)
  willCollapse.value = props.collapsible && raw < props.min - COLLAPSE_SLACK
  emit('update:width', clamp(raw))
}

function onPointerUp(e: PointerEvent): void {
  if (!dragging.value) return
  ;(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)
  dragging.value = false
  if (willCollapse.value) {
    willCollapse.value = false
    emit('update:width', startWidth)
    emit('collapse')
  }
}

function onKey(e: KeyboardEvent): void {
  let next: number | null = null
  if (e.key === 'ArrowLeft') next = props.width + STEP
  else if (e.key === 'ArrowRight') next = props.width - STEP
  else if (e.key === 'Home') next = props.max
  else if (e.key === 'End') next = props.min
  else if (e.key === 'Enter' && props.collapsible) {
    e.preventDefault()
    emit('collapse')
    return
  }
  if (next === null) return
  e.preventDefault()
  emit('update:width', clamp(next))
}
</script>

<template>
  <div
    class="ps"
    :class="{ 'ps--drag': dragging, 'ps--collapse': willCollapse }"
    role="separator"
    aria-orientation="vertical"
    tabindex="0"
    :aria-label="label"
    :aria-controls="controls"
    :aria-valuenow="width"
    :aria-valuemin="min"
    :aria-valuemax="max"
    :title="collapsible ? 'Drag to resize · drag right to hide · double-click to reset' : 'Drag to resize · double-click to reset'"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerUp"
    @dblclick="emit('update:width', defaultWidth)"
    @keydown="onKey"
  ></div>
</template>

<style scoped>
.ps {
  position: relative;
  flex: none;
  width: 7px;
  margin: 0 -3px;
  z-index: 2;
  cursor: col-resize;
  touch-action: none;
}
/* Garis tipis di tengah area seret, sebagai penanda. */
.ps::after {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  left: 3px;
  width: 1px;
  background: var(--border);
  transition: background var(--dur-fast) var(--ease), width var(--dur-fast) var(--ease);
}
.ps:hover::after,
.ps--drag::after { left: 2px; width: 3px; background: var(--accent); }
.ps--collapse::after { background: var(--amber); }
.ps:focus-visible { outline: none; }
.ps:focus-visible::after { left: 2px; width: 3px; background: var(--accent); }
@media (prefers-reduced-motion: reduce) {
  .ps::after { transition: none; }
}
</style>
