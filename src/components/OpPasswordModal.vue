<script setup lang="ts">
import { ref, reactive, computed, watch } from "vue"
import {
  NModal,
  NCard,
  NForm,
  NFormItem,
  NInput,
  NButton,
  NSpace,
  NText,
  useMessage,
} from "naive-ui"
import { useI18n } from "vue-i18n"
import { setOpPassword, changeOpPassword, clearOpPassword } from "@/api/security"
import { ApiError } from "@/api/client"

type Mode = "verify" | "setup" | "change" | "clear"

const props = defineProps<{
  show: boolean
  mode: Mode
}>()

const emit = defineEmits<{
  "update:show": [value: boolean]
  /** verify 模式：把用户输入的密码传回父组件 */
  verified: [password: string]
  /** setup / change / clear 完成后触发 */
  done: []
}>()

const { t } = useI18n()
const message = useMessage()

const form = reactive({
  password: "",
  oldPassword: "",
  newPassword: "",
  confirmPassword: "",
})
const loading = ref(false)

const title = computed(() => {
  switch (props.mode) {
    case "verify":
      return t("hosts.form.security.verifyTitle")
    case "setup":
      return t("hosts.form.security.setupTitle")
    case "change":
      return t("hosts.form.security.changeTitle")
    case "clear":
      return t("hosts.form.security.clearTitle")
  }
})

const description = computed(() => {
  switch (props.mode) {
    case "verify":
      return t("hosts.form.security.verifyDesc")
    case "setup":
      return t("hosts.form.security.setupDesc")
    default:
      return ""
  }
})

watch(
  () => props.show,
  (v) => {
    if (v) {
      form.password = ""
      form.oldPassword = ""
      form.newPassword = ""
      form.confirmPassword = ""
    }
  },
)

function cancel() {
  emit("update:show", false)
}

function errMsg(e: unknown): string {
  if (e instanceof ApiError) return e.message
  return String(e)
}

async function submit() {
  if (props.mode === "verify") {
    if (!form.password) {
      message.warning(t("hosts.form.security.empty"))
      return
    }
    emit("verified", form.password)
    emit("update:show", false)
    return
  }

  if (props.mode === "setup") {
    if (!form.newPassword) {
      message.warning(t("hosts.form.security.empty"))
      return
    }
    if (form.newPassword !== form.confirmPassword) {
      message.warning(t("hosts.form.security.mismatch"))
      return
    }
    loading.value = true
    try {
      await setOpPassword(form.newPassword)
      message.success(t("hosts.form.security.setSuccess"))
      emit("done")
      emit("update:show", false)
    } catch (e) {
      message.error(errMsg(e))
    } finally {
      loading.value = false
    }
    return
  }

  if (props.mode === "change") {
    if (!form.oldPassword || !form.newPassword) {
      message.warning(t("hosts.form.security.empty"))
      return
    }
    if (form.newPassword !== form.confirmPassword) {
      message.warning(t("hosts.form.security.mismatch"))
      return
    }
    loading.value = true
    try {
      await changeOpPassword(form.oldPassword, form.newPassword)
      message.success(t("hosts.form.security.changeSuccess"))
      emit("done")
      emit("update:show", false)
    } catch (e) {
      message.error(errMsg(e))
    } finally {
      loading.value = false
    }
    return
  }

  if (props.mode === "clear") {
    if (!form.password) {
      message.warning(t("hosts.form.security.empty"))
      return
    }
    loading.value = true
    try {
      await clearOpPassword(form.password)
      message.success(t("hosts.form.security.clearSuccess"))
      emit("done")
      emit("update:show", false)
    } catch (e) {
      message.error(errMsg(e))
    } finally {
      loading.value = false
    }
    return
  }
}
</script>

<template>
  <NModal
    :show="show"
    @update:show="(v: boolean) => emit('update:show', v)"
    :mask-closable="!loading"
  >
    <NCard
      style="width: 400px; max-width: 90vw"
      :title="title"
      size="small"
      :bordered="false"
      role="dialog"
      aria-modal="true"
    >
      <NForm label-placement="top" @submit.prevent="submit">
        <NText v-if="description" depth="3" style="display: block; margin-bottom: 12px; font-size: 13px">
          {{ description }}
        </NText>

        <NFormItem v-if="mode === 'change'" :label="t('hosts.form.security.oldPassword')">
          <NInput
            v-model:value="form.oldPassword"
            type="password"
            show-password-on="click"
            :placeholder="t('hosts.form.security.passwordPlaceholder')"
          />
        </NFormItem>

        <NFormItem
          v-if="mode === 'setup' || mode === 'change'"
          :label="mode === 'change' ? t('hosts.form.security.newPassword') : t('hosts.form.security.password')"
        >
          <NInput
            v-model:value="form.newPassword"
            type="password"
            show-password-on="click"
            :placeholder="t('hosts.form.security.newPasswordPlaceholder')"
          />
        </NFormItem>

        <NFormItem
          v-if="mode === 'setup' || mode === 'change'"
          :label="t('hosts.form.security.confirmPassword')"
        >
          <NInput
            v-model:value="form.confirmPassword"
            type="password"
            show-password-on="click"
            :placeholder="t('hosts.form.security.confirmPasswordPlaceholder')"
            @keyup.enter="submit"
          />
        </NFormItem>

        <NFormItem
          v-if="mode === 'verify' || mode === 'clear'"
          :label="t('hosts.form.security.password')"
        >
          <NInput
            v-model:value="form.password"
            type="password"
            show-password-on="click"
            :placeholder="t('hosts.form.security.passwordPlaceholder')"
            @keyup.enter="submit"
          />
        </NFormItem>

        <NSpace justify="end">
          <NButton @click="cancel" :disabled="loading">
            {{ t("hosts.form.security.cancel") }}
          </NButton>
          <NButton type="primary" :loading="loading" @click="submit">
            {{ t("hosts.form.security.confirm") }}
          </NButton>
        </NSpace>
      </NForm>
    </NCard>
  </NModal>
</template>
