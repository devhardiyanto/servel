<script setup lang="ts">
// Add project (v3.4, canvas B1–B5): pilih folder dengan dialog OS, lalu modal
// menampilkan hasil deteksi, cek port, dan Save / Save & start. Project baru
// tersimpan hanya setelah Save; Cancel tidak meninggalkan apa pun.
import { ref, watch } from 'vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import UiModal from '@/components/ui/UiModal.vue'
import UiButton from '@/components/ui/UiButton.vue'
import AddProjectForm from '@/components/AddProjectForm.vue'
import { useProjectActions } from '@/composables/useProjectActions'
import { useToast } from '@/composables/useToast'

const emit = defineEmits<{ added: [id: string] }>()
const { adding } = useProjectActions()
const toast = useToast()

const path = ref('')
const form = ref<InstanceType<typeof AddProjectForm> | null>(null)
const pickError = ref('')

watch(adding, async (on) => {
  if (!on) return
  pickError.value = ''
  path.value = ''
  try {
    const picked = await openDialog({ directory: true, multiple: false, title: 'Add project folder' })
    if (typeof picked === 'string') path.value = picked
    else adding.value = false
  } catch (err) {
    pickError.value = String(err)
    adding.value = false
    toast.show(`Could not open the folder picker: ${pickError.value}`)
  }
})

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
  <UiModal :open="adding && !!path" title="Add project" @close="close">
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
