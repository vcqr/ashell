<script setup lang="ts">
import { ref, watch } from "vue"
import { NModal, NCard, NInput, NButton, NSpace, NFormItem } from "naive-ui"
import { useI18n } from "vue-i18n"

interface Props {
  open: boolean
  oldName: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  "update:open": [value: boolean]
  submit: [newName: string]
}>()

const { t } = useI18n()

const value = ref("")
const errMsg = ref("")

watch(
  () => props.open,
  (v) => {
    if (v) {
      value.value = props.oldName
      errMsg.value = ""
    }
  },
)

function validate(): boolean {
  const n = value.value.trim()
  if (!n) {
    errMsg.value = t("sftp.rename.nameRequired")
    return false
  }
  if (n.includes("/")) {
    errMsg.value = t("sftp.rename.nameContainsSlash")
    return false
  }
  if (n === props.oldName) {
    errMsg.value = t("sftp.rename.sameName")
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
  emit("submit", value.value.trim())
}
</script>

<template>
  <NModal :show="props.open" :mask-closable="false" @update:show="(v) => emit('update:open', v)">
    <NCard
      :title="t('sftp.rename.title')"
      style="width: 480px"
      :bordered="false"
      size="small"
      role="dialog"
      aria-modal="true"
    >
      <div class="hint">{{ t("sftp.rename.originalName", { name: props.oldName }) }}</div>
      <NFormItem :show-label="false" :feedback="errMsg" :validation-status="errMsg ? 'error' : undefined">
        <NInput
          v-model:value="value"
          :placeholder="t('sftp.rename.placeholder')"
          autofocus
          @keyup.enter="onSubmit"
        />
      </NFormItem>
      <template #footer>
        <NSpace justify="end">
          <NButton size="small" @click="close">{{ t("sftp.rename.cancel") }}</NButton>
          <NButton size="small" type="primary" @click="onSubmit">{{ t("sftp.rename.confirm") }}</NButton>
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
