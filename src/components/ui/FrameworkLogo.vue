<script setup lang="ts">
// Logo framework dekoratif (aria-hidden): nama jenisnya selalu tertulis di
// dekatnya. Tidak dirender bila jenisnya tidak punya logo.
import { computed } from 'vue'
import { frameworkColor, frameworkIcon } from '@/utils/framework'

const props = withDefaults(defineProps<{ kind?: string | null; size?: number }>(), { kind: '', size: 12 })
const icon = computed(() => frameworkIcon(props.kind))
</script>

<template>
  <svg
    v-if="icon"
    class="fw-logo"
    viewBox="0 0 24 24"
    :width="size"
    :height="size"
    aria-hidden="true"
    focusable="false"
    :style="{ color: frameworkColor(icon) }"
  ><path :d="icon.path" /></svg>
</template>

<style scoped>
.fw-logo { flex: none; fill: currentColor; }
</style>
