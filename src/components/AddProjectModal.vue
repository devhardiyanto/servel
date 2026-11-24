<script setup lang="ts">
// Add project: pilih folder dengan dialog OS, lalu modal
// menampilkan hasil deteksi, cek port, dan Save / Save & start. Project baru
// tersimpan hanya setelah Save; Cancel tidak meninggalkan apa pun.
// Folder yang sudah menjadi project dikenali saat dipilih (canvas B13).
import { computed, ref, watch } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import UiModal from '@/components/ui/UiModal.vue'
import UiButton from '@/components/ui/UiButton.vue'
import AddProjectForm from '@/components/AddProjectForm.vue'
import { useProjectActions } from '@/composables/useProjectActions'
import { useToast } from '@/composables/useToast'
import { useConfig } from '@/composables/useConfig'

const emit = defineEmits<{ added: [id: string] }>()
const { adding, selectRequest } = useProjectActions()
const { config } = useConfig()
const toast = useToast()

const path = ref('')
const form = ref<InstanceType<typeof AddProjectForm> | null>(null)
const pickError = ref('')

const norm = (p: string): string => p.replace(/[\\/]+/g, '/').replace(/\/+$/, '').toLowerCase()
/** Project yang sudah memakai folder terpilih. */
const existing = computed(() => (path.value ? config.value.projects.find((p) => p.path && norm(p.path) === norm(path.value)) ?? null : null))

async function pick(): Promise<void> {
  pickError.value = ''
  path.value = ''
  try {
    const picked = await openDialog({ directory: true, multiple: false, title: 'Add project folder' })
    if (typeof picked === 'string') path.value = picked
    else adding.value = false
  } catch (err) {
    pickError.value = String(err)
    adding.value = false
    toast.error(`Could not open the folder picker: ${pickError.value}`)
  }
}

watch(adding, (on) => {
  if (on) void pick()
})

function showExisting(): void {
  const id = existing.value?.id
  close()
  if (id) selectRequest.value = id
}

function close(): void {
  adding.value = false
  path.value = ''
}

async function save(start: boolean): Promise<void> {
  const p = await form.value?.save(start)
  if (!p) return
  emit('added', p.id)
  toast.show(start ? `Starting ${p.name}` : `Added ${p.name}`)
  close()
}
</script>

<template>
  <UiModal v-if="existing" :open="adding && !!path" :title="`${existing.name} already uses this folder`" size="sm" @close="close">
    <p>
      <code class="apm-path">{{ path }}</code> is already the project {{ existing.name }}.
      A folder can belong to one project only.
    </p>
    <template #footer>
      <UiButton variant="quiet" size="md" @click="close">Cancel</UiButton>
      <UiButton size="md" @click="pick">Choose another folder</UiButton>
      <UiButton variant="primary" size="md" data-autofocus @click="showExisting">Show {{ existing.name }}</UiButton>
    </template>
  </UiModal>
  <UiModal v-else :open="adding && !!path" title="Add project" @close="close">
    <AddProjectForm v-if="path" ref="form" :key="path" :path="path" />
    <template #footer>
      <UiButton variant="quiet" size="md" @click="close">Cancel</UiButton>
      <UiButton size="md" :disabled="!form?.ready || form?.saving" @click="save(false)">Save</UiButton>
      <UiButton
        variant="primary"
        size="md"
        :disabled="!form?.ready || !form?.canStart || form?.saving"
        @click="save(true)"
      >Save &amp; start</UiButton>
    </template>
  </UiModal>
</template>

<style scoped>
.apm-path { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--text); overflow-wrap: anywhere; }
</style>
