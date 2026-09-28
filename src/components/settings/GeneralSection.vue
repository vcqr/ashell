<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NSwitch,
  type SelectOption,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  localePreference,
  setLocalePreference,
  type LocalePreference,
} from "@/locales";
import { isTauri } from "@/utils/platform";
import { devtoolsEnabled, setDevtoolsEnabled } from "@/utils/devtools";
import { useProxyStore, type ProxyMode } from "@/stores/proxy";

const { t } = useI18n();

const languageOptions: SelectOption[] = [
  { label: "简体中文", value: "zh-CN" },
  { label: "English", value: "en-US" },
  { label: "Auto (Follow system)", value: "auto" },
];

function onChange(value: LocalePreference) {
  setLocalePreference(value);
}

const devtoolsOn = ref(devtoolsEnabled());

async function onDevtoolsChange(value: boolean) {
  devtoolsOn.value = value;
  try {
    await setDevtoolsEnabled(value);
  } catch {
    // 后端调用失败（如前端新于后端）时回退开关，避免 UI 与实际状态不一致
    devtoolsOn.value = !value;
  }
}

const proxyStore = useProxyStore();

onMounted(() => {
  proxyStore.load();
});

const proxyModeOptions = computed<SelectOption[]>(() => [
  { label: t("settings.general.proxyFollowSystem"), value: "system" },
  { label: t("settings.general.proxyDirect"), value: "direct" },
  { label: t("settings.general.proxyCustom"), value: "custom" },
]);

const proxyError = ref("");

function onProxyModeChange(value: ProxyMode) {
  proxyError.value = "";
  proxyStore.save({ mode: value }).catch(() => {
    proxyError.value = t("settings.general.proxySaveFailed");
  });
}

const proxyUrlPattern = /^(https?|socks5h?):\/\//i;

function saveProxyUrl() {
  const url = proxyStore.url.trim();
  if (url && !proxyUrlPattern.test(url)) {
    proxyError.value = t("settings.general.proxyUrlInvalid");
    return;
  }
  proxyError.value = "";
  proxyStore.save({ url }).catch(() => {
    proxyError.value = t("settings.general.proxySaveFailed");
  });
}

function saveProxyNoProxy() {
  proxyError.value = "";
  proxyStore
    .save({ noProxy: proxyStore.noProxy.trim() })
    .catch(() => {
      proxyError.value = t("settings.general.proxySaveFailed");
    });
}
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.general.title") }}</div>
    <NForm label-placement="top" size="small" :show-feedback="false">
      <NFormItem :label="t('settings.general.language')">
        <NSelect
          :value="localePreference"
          :options="languageOptions"
          style="width: 240px"
          @update:value="onChange"
        />
      </NFormItem>
      <p class="settings-hint">{{ t("settings.general.languageDesc") }}</p>
    </NForm>

    <div v-if="isTauri" class="sub-block">
      <div class="settings-section-title">
        {{ t("settings.general.proxyTitle") }}
      </div>
      <NForm label-placement="top" size="small" :show-feedback="false">
        <NFormItem :label="t('settings.general.proxyMode')">
          <NSelect
            :value="proxyStore.mode"
            :options="proxyModeOptions"
            style="width: 240px"
            @update:value="onProxyModeChange"
          />
        </NFormItem>
        <template v-if="proxyStore.mode === 'custom'">
          <NFormItem :label="t('settings.general.proxyUrl')">
            <NInput
              v-model:value="proxyStore.url"
              :placeholder="t('settings.general.proxyUrlPlaceholder')"
              style="width: 320px"
              @blur="saveProxyUrl"
              @keyup.enter="saveProxyUrl"
            />
          </NFormItem>
          <NFormItem :label="t('settings.general.proxyNoProxy')">
            <NInput
              v-model:value="proxyStore.noProxy"
              :placeholder="t('settings.general.proxyNoProxyPlaceholder')"
              style="width: 320px"
              @blur="saveProxyNoProxy"
              @keyup.enter="saveProxyNoProxy"
            />
          </NFormItem>
          <p class="settings-hint">
            {{ t("settings.general.proxyNoProxyDesc") }}
          </p>
        </template>
        <p v-if="proxyError" class="settings-hint proxy-error">
          {{ proxyError }}
        </p>
        <p class="settings-hint">{{ t("settings.general.proxyDesc") }}</p>
      </NForm>
    </div>

    <div v-if="isTauri" class="sub-block">
      <div class="settings-section-title">
        {{ t("settings.general.developerTitle") }}
      </div>
      <NForm label-placement="top" size="small" :show-feedback="false">
        <NFormItem :label="t('settings.general.devtools')">
          <NSwitch :value="devtoolsOn" @update:value="onDevtoolsChange" />
        </NFormItem>
        <p class="settings-hint">{{ t("settings.general.devtoolsDesc") }}</p>
      </NForm>
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

.sub-block {
  margin-top: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* show-feedback 关闭后表单项之间没有空隙，补回呼吸感 */
.sub-block :deep(.n-form-item) {
  margin-bottom: 14px;
}

.settings-hint + .settings-hint {
  margin-top: 12px;
}

.settings-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.6;
}

.proxy-error {
  color: var(--ashell-danger, #e5484d);
}
</style>
