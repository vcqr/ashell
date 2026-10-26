<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
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
  custom_template: "",
});

const platformOptions = computed(() =>
  (
    ["feishu", "wecom", "dingtalk", "custom"] as NotifyPlatform[]
  ).map((p) => ({ label: t(`settings.notify.platform_${p}`), value: p })),
);

/** 各平台机器人的官方 webhook 域名，用于保存时校验平台与地址匹配 */
const PLATFORM_HOSTS: Record<NotifyPlatform, string[]> = {
  feishu: ["open.feishu.cn", "open.larksuite.com"],
  wecom: ["qyapi.weixin.qq.com"],
  dingtalk: ["oapi.dingtalk.com"],
  custom: [],
};

// 只拦截「地址明确属于其他平台」的情况；自建/反代等未知域名放行。
// 自定义平台不校验（任意域名合法）
function webhookBelongsToOtherPlatform(webhook: string): boolean {
  if (config.value.platform === "custom") return false;
  if (!webhook) return false;
  let host = "";
  try {
    host = new URL(webhook).hostname;
  } catch {
    return false;
  }
  return Object.entries(PLATFORM_HOSTS)
    .filter(([p]) => p !== config.value.platform)
    .some(([, hosts]) => hosts.some((h) => host === h || host.endsWith(`.${h}`)));
}

// 企业微信/自定义平台没有加签机制，不显示密钥输入
const needSecret = computed(
  () =>
    config.value.platform !== "wecom" && config.value.platform !== "custom",
);

/** 自定义平台的消息体模板示例（也是切换到自定义时的预填默认值） */
const CUSTOM_TEMPLATE_EXAMPLE =
  '{"msgtype":"text","text":{"content":"【ashell】{title}\\n会话: {sid}\\n\\n{content}"}}';

// 切到自定义平台且模板为空时预填示例，避免空模板发送报错
watch(
  () => config.value.platform,
  (p) => {
    if (p === "custom" && !config.value.custom_template.trim()) {
      config.value.custom_template = CUSTOM_TEMPLATE_EXAMPLE;
    }
  },
);

/** 切换平台视为重选目标机器人：清空上一平台的地址/密钥/模板，避免跨平台残留 */
function onPlatformChange(p: NotifyPlatform) {
  if (p === config.value.platform) return;
  config.value.platform = p;
  config.value.webhook = "";
  config.value.secret = "";
  config.value.custom_template = "";
}

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
    custom_template: config.value.custom_template.trim(),
  };
}

async function handleSave() {
  const payload = buildPayload();
  if (payload.enabled && !payload.webhook) {
    message.warning(t("settings.notify.webhookRequired"));
    return;
  }
  if (webhookBelongsToOtherPlatform(payload.webhook)) {
    message.error(t("settings.notify.platformMismatch"));
    return;
  }
  if (payload.platform === "custom" && !payload.custom_template) {
    message.warning(t("settings.notify.templateRequired"));
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
  if (webhookBelongsToOtherPlatform(payload.webhook)) {
    message.error(t("settings.notify.platformMismatch"));
    return;
  }
  if (payload.platform === "custom" && !payload.custom_template) {
    message.warning(t("settings.notify.templateRequired"));
    return;
  }
  testing.value = true;
  try {
    await testNotifyConnection({
      platform: payload.platform,
      webhook: payload.webhook,
      secret: payload.secret,
      custom_template: payload.custom_template,
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
          :value="config.platform"
          :options="platformOptions"
          :disabled="loading"
          style="max-width: 320px"
          @update:value="onPlatformChange"
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
        v-if="config.platform === 'custom'"
        :label="t('settings.notify.customTemplate')"
        :feedback="t('settings.notify.customTemplateHint')"
      >
        <NInput
          v-model:value="config.custom_template"
          type="textarea"
          :placeholder="CUSTOM_TEMPLATE_EXAMPLE"
          :autosize="{ minRows: 6, maxRows: 14 }"
          :disabled="loading"
          class="template-input"
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

.template-input :deep(textarea) {
  font-family: "SF Mono", Menlo, Consolas, monospace;
  font-size: 12px;
}
</style>
