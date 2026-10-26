<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  NButton,
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NSwitch,
  useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  getNotifyConfig,
  saveNotifyConfig,
  testNotifyConnection,
  type NotifyConfig,
  type NotifyPlatform,
} from "@/api/notify";

const { t } = useI18n();
const message = useMessage();

const loading = ref(false);
const saving = ref(false);
const testing = ref(false);

const config = ref<NotifyConfig>({
  enabled: false,
  platform: "feishu",
  webhook: "",
  secret: "",
});

const platformOptions = computed(() =>
  (
    ["feishu", "wecom", "dingtalk"] as NotifyPlatform[]
  ).map((p) => ({ label: t(`settings.notify.platform_${p}`), value: p })),
);

// 企业微信机器人没有加签机制，不显示密钥输入
const needSecret = computed(() => config.value.platform !== "wecom");

const secretHint = computed(() =>
  config.value.platform === "dingtalk"
    ? t("settings.notify.secretHintDingtalk")
    : t("settings.notify.secretHintFeishu"),
);

async function loadConfig() {
  loading.value = true;
  try {
    config.value = await getNotifyConfig();
  } catch {
    // 加载失败按默认值展示，允许用户重新填写保存
  } finally {
    loading.value = false;
  }
}

function buildPayload(): NotifyConfig {
  return {
    enabled: config.value.enabled,
    platform: config.value.platform,
    webhook: config.value.webhook.trim(),
    secret: config.value.secret.trim(),
  };
}

async function handleSave() {
  const payload = buildPayload();
  if (payload.enabled && !payload.webhook) {
    message.warning(t("settings.notify.webhookRequired"));
    return;
  }
  saving.value = true;
  try {
    await saveNotifyConfig(payload);
    message.success(t("settings.notify.saveSuccess"));
  } catch (e) {
    message.error(t("settings.notify.saveFailed", { error: String(e) }));
  } finally {
    saving.value = false;
  }
}

async function handleTest() {
  const payload = buildPayload();
  if (!payload.webhook) {
    message.warning(t("settings.notify.webhookRequired"));
    return;
  }
  testing.value = true;
  try {
    await testNotifyConnection({
      platform: payload.platform,
      webhook: payload.webhook,
      secret: payload.secret,
    });
    message.success(t("settings.notify.testSuccess"));
  } catch (e) {
    message.error(t("settings.notify.testFailed", { error: String(e) }));
  } finally {
    testing.value = false;
  }
}

onMounted(loadConfig);
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.notify.title") }}</div>
    <p class="settings-hint">{{ t("settings.notify.description") }}</p>
    <NForm label-placement="top" require-mark-placement="left">
      <NFormItem :label="t('settings.notify.enabled')">
        <NSwitch v-model:value="config.enabled" />
      </NFormItem>
      <NFormItem :label="t('settings.notify.platform')">
        <NSelect
          v-model:value="config.platform"
          :options="platformOptions"
          :disabled="loading"
          style="max-width: 320px"
        />
      </NFormItem>
      <NFormItem
        :label="t('settings.notify.webhook')"
        :feedback="t('settings.notify.webhookHint')"
      >
        <NInput
          v-model:value="config.webhook"
          :placeholder="t('settings.notify.webhookPlaceholder')"
          :disabled="loading"
        />
      </NFormItem>
      <NFormItem
        v-if="needSecret"
        :label="t('settings.notify.secret')"
        :feedback="secretHint"
      >
        <NInput
          v-model:value="config.secret"
          :placeholder="t('settings.notify.secretPlaceholder')"
          :disabled="loading"
        />
      </NFormItem>
      <NFormItem :show-label="false">
        <div class="form-actions">
          <NButton
            size="small"
            type="primary"
            :loading="saving"
            @click="handleSave"
          >
            {{ t("settings.notify.save") }}
          </NButton>
          <NButton size="small" :loading="testing" @click="handleTest">
            {{ t("settings.notify.testConnection") }}
          </NButton>
        </div>
      </NFormItem>
    </NForm>
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

.settings-hint {
  margin: 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.5;
}

.form-actions {
  display: flex;
  gap: 8px;
}
</style>
