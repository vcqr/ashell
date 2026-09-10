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
import { listKnownHosts, deleteKnownHost, type KnownHost } from "@/api/knownHosts"
import OpPasswordModal from "@/components/OpPasswordModal.vue"

const { t } = useI18n()
const message = useMessage()
const dialog = useDialog()

const isSet = ref(false)
const loading = ref(false)

const modalShow = ref(false)
const modalMode = ref<"setup" | "change" | "clear">("setup")

const knownHosts = ref<KnownHost[]>([])
const knownHostsLoading = ref(false)

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

async function refreshKnownHosts() {
  knownHostsLoading.value = true
  try {
    knownHosts.value = await listKnownHosts()
  } catch {
    // 非 Tauri 环境（浏览器 dev）下忽略
  } finally {
    knownHostsLoading.value = false
  }
}

onMounted(() => {
  void refresh()
  void refreshKnownHosts()
})

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

function onDeleteKnownHost(item: KnownHost) {
  const label = `${item.addr}:${item.port}`
  dialog.warning({
    title: t("settings.security.knownHostsDeleteTitle"),
    content: t("settings.security.knownHostsDeleteConfirm", { label }),
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        await deleteKnownHost(item.id)
        message.success(t("settings.security.knownHostsDeleted"))
        void refreshKnownHosts()
      } catch (e) {
        message.error(t("settings.security.knownHostsDeleteFailed", { error: String(e) }))
      }
    },
  })
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
        <NButton v-if="!isSet" size="small" type="primary" :loading="loading" @click="onSet">
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

    <div class="known-hosts-block">
      <div class="known-hosts-title">{{ t("settings.security.knownHosts") }}</div>
      <NText depth="3" style="font-size: 12px; line-height: 1.6; display: block">
        {{ t("settings.security.knownHostsHint") }}
      </NText>
      <div v-if="knownHosts.length === 0" class="known-hosts-empty">
        {{ t("settings.security.knownHostsEmpty") }}
      </div>
      <div v-else class="known-hosts-list">
        <div v-for="item in knownHosts" :key="item.id" class="known-host-row">
          <div class="known-host-info">
            <span class="known-host-addr">{{ item.addr }}:{{ item.port }}</span>
            <NTag size="tiny" :bordered="true">{{ item.key_type }}</NTag>
          </div>
          <span class="known-host-fp">{{ item.fingerprint }}</span>
          <NButton
            size="tiny"
            quaternary
            type="error"
            :loading="knownHostsLoading"
            @click="onDeleteKnownHost(item)"
          >
            {{ t("common.delete") }}
          </NButton>
        </div>
      </div>
    </div>
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

.known-hosts-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 10px;
  border-top: 1px solid var(--ashell-border, rgba(255, 255, 255, 0.08));
}

.known-hosts-title {
  font-size: 12px;
  font-weight: 500;
  color: var(--ashell-text);
}

.known-hosts-empty {
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  padding: 4px 0;
}

.known-hosts-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 260px;
  overflow-y: auto;
}

.known-host-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 6px;
}

.known-host-row:hover {
  background: var(--ashell-hover);
}

.known-host-info {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}

.known-host-addr {
  font-size: 13px;
  color: var(--ashell-text);
}

.known-host-fp {
  flex: 1;
  font-size: 11px;
  font-family: var(--ashell-mono, monospace);
  color: var(--ashell-text-muted, #98a2b3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
</style>
