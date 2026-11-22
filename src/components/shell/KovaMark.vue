<script setup lang="ts">
// Logo K: tiga garis, kaki bawah memakai warna aksen.
withDefaults(defineProps<{ size?: number }>(), { size: 22 })
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 64 64" aria-hidden="true">
    <rect x="2" y="2" width="60" height="60" rx="14" fill="var(--bg-sunken)" />
    <path d="M21 15V49" stroke="var(--text)" stroke-width="7" stroke-linecap="round" />
    <path d="M44 15L28 32" stroke="var(--text)" stroke-width="7" stroke-linecap="round" />
    <path d="M28 32L44 49" stroke="var(--accent)" stroke-width="7" stroke-linecap="round" />
  </svg>
</template>
