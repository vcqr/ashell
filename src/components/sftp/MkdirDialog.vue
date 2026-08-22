<script setup lang="ts">
import { computed, ref, watch } from "vue"
import { NModal, NCard, NInput, NButton, NSpace, NFormItem } from "naive-ui"
import { useI18n } from "vue-i18n"

interface Props {
  open: boolean
  mode: "mkdir" | "touch"
  currentPath: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  "update:open": [value: boolean]
  submit: [name: string]
}>()

const { t } = useI18n()

const name = ref("")
const errMsg = ref("")

const title = computed(() =>
  props.mode === "mkdir" ? t("sftp.mkdir.newFolder") : t("sftp.mkdir.newFile"),
)
const placeholder = computed(() =>
  props.mode === "mkdir" ? t("sftp.mkdir.folderPlaceholder") : t("sftp.mkdir.filePlaceholder"),
)

watch(
  () => props.open,
  (v) => {
    if (v) {
      name.value = ""
      errMsg.value = ""
    }
  },
)

function validate(): boolean {
  const n = name.value.trim()
  if (!n) {
    errMsg.value = t("sftp.mkdir.nameRequired")
    return false
  }
  if (n.includes("/")) {
    errMsg.value = t("sftp.mkdir.nameContainsSlash")
    return false
  }
  errMsg.value = ""
  return true
}

function close() {
  emit("update:open", false)
}

function onSubmit() {
  if (!validate()) return
  emit("submit", name.value.trim())
}
</script>

<template>
  <NModal :show="props.open" :mask-closable="false" @update:show="(v) => emit('update:open', v)">
    <NCard
      :title="title"
      style="width: 480px"
      :bordered="false"
      size="small"
      role="dialog"
      aria-modal="true"
    >
      <div class="hint">{{ t("sftp.mkdir.currentDir", { path: props.currentPath }) }}</div>
      <NFormItem :show-label="false" :feedback="errMsg" :validation-status="errMsg ? 'error' : undefined">
        <NInput
          v-model:value="name"
          :placeholder="placeholder"
          autofocus
          @keyup.enter="onSubmit"
        />
      </NFormItem>
      <template #footer>
        <NSpace justify="end">
          <NButton size="small" @click="close">{{ t("sftp.mkdir.cancel") }}</NButton>
          <NButton size="small" type="primary" @click="onSubmit">{{ t("sftp.mkdir.confirm") }}</NButton>
        </NSpace>
      </template>
    </NCard>
  </NModal>
</template>

<style scoped>
.hint {
  margin-bottom: 10px;
  color: var(--ashell-text-subtle);
  font-size: 12px;
  word-break: break-all;
}
</style>
