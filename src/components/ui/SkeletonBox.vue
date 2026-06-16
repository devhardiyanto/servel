<script setup lang="ts">
import { computed } from 'vue'

type Variant = 'text' | 'chip' | 'box'

const props = withDefaults(
  defineProps<{
    width?: string
    height?: string
    radius?: string
    variant?: Variant
  }>(),
  {
    width: '100%',
    height: '12px',
    radius: '4px',
    variant: 'text',
  },
)

const style = computed(() => ({
  width: props.width,
  height: props.height,
  borderRadius: props.radius,
}))
</script>

<template>
  <span
    class="skeleton"
    :class="['skeleton--' + variant]"
    :style="style"
    aria-busy="true"
    aria-label="Loading"
  />
</template>
