<script setup lang="ts">
import { ref, onMounted } from "vue"
import {
  NButton,
  NSpace,
  NText,
  NTag,
  useMessage,
  useDialog,
} from "naive-ui"
import { useI18n } from "vue-i18n"
import { getOpPasswordStatus } from "@/api/security"
import OpPasswordModal from "@/components/OpPasswordModal.vue"

const { t } = useI18n()
const message = useMessage()
const dialog = useDialog()

const isSet = ref(false)
const loading = ref(false)

const modalShow = ref(false)
const modalMode = ref<"setup" | "change" | "clear">("setup")

async function refresh() {
  loading.value = true
  try {
    const status = await getOpPasswordStatus()
    isSet.value = status.set
  } catch (e) {
    message.error(String(e))
  } finally {
    loading.value = false
  }
}

onMounted(refresh)

function onSet() {
  modalMode.value = "setup"
  modalShow.value = true
}

function onChange() {
  modalMode.value = "change"
  modalShow.value = true
}

function onClear() {
  dialog.warning({
    title: t("settings.security.clearTitle"),
    content: t("settings.security.clearConfirm"),
    positiveText: t("settings.security.clearBtn"),
    negativeText: t("common.cancel"),
    onPositiveClick: () => {
      modalMode.value = "clear"
      modalShow.value = true
    },
  })
}

function onModalDone() {
  refresh()
}
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.security.title") }}</div>

    <div class="security-row">
      <div class="security-status">
        <NText depth="2" style="font-size: 13px">
          {{ t("settings.security.status") }}
        </NText>
        <NTag :type="isSet ? 'success' : 'default'" size="small" round>
          {{ isSet ? t("settings.security.statusSet") : t("settings.security.statusNotSet") }}
        </NTag>
      </div>

      <NSpace>
        <NButton v-if="!isSet" size="small" type="primary" @click="onSet" :loading="loading">
          {{ t("settings.security.setBtn") }}
        </NButton>
        <template v-else>
          <NButton size="small" @click="onChange">
            {{ t("settings.security.changeBtn") }}
          </NButton>
          <NButton size="small" quaternary type="error" @click="onClear">
            {{ t("settings.security.clearBtn") }}
          </NButton>
        </template>
      </NSpace>
    </div>

    <NText depth="3" style="font-size: 12px; line-height: 1.6; display: block">
      {{ t("settings.security.hint") }}
    </NText>

    <OpPasswordModal
      v-model:show="modalShow"
      :mode="modalMode"
      @done="onModalDone"
    />
  </section>
</template>

<style scoped>
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.settings-section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
}

.security-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.security-status {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
