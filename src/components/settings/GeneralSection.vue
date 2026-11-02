<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NForm, NInput, NSelect, NSwitch, type SelectOption } from "naive-ui";
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
    <NForm size="small" :show-feedback="false">
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.general.language") }}</div>
            <div class="setting-row-desc">{{ t("settings.general.languageDesc") }}</div>
          </div>
          <div class="setting-row-control w-select">
            <NSelect
              :value="localePreference"
              :options="languageOptions"
              @update:value="onChange"
            />
          </div>
        </div>
      </div>

      <template v-if="isTauri">
        <div class="settings-subgroup">{{ t("settings.general.proxyTitle") }}</div>
        <div class="settings-block">
          <div class="setting-row">
            <div class="setting-row-main">
              <div class="setting-row-label">{{ t("settings.general.proxyMode") }}</div>
            </div>
            <div class="setting-row-control w-select">
              <NSelect
                :value="proxyStore.mode"
                :options="proxyModeOptions"
                @update:value="onProxyModeChange"
              />
            </div>
          </div>
          <template v-if="proxyStore.mode === 'custom'">
            <div class="setting-row">
              <div class="setting-row-main">
                <div class="setting-row-label">{{ t("settings.general.proxyUrl") }}</div>
              </div>
              <div class="setting-row-control w-url">
                <NInput
                  v-model:value="proxyStore.url"
                  :placeholder="t('settings.general.proxyUrlPlaceholder')"
                  @blur="saveProxyUrl"
                  @keyup.enter="saveProxyUrl"
                />
              </div>
            </div>
            <div class="setting-row">
              <div class="setting-row-main">
                <div class="setting-row-label">{{ t("settings.general.proxyNoProxy") }}</div>
                <div class="setting-row-desc">{{ t("settings.general.proxyNoProxyDesc") }}</div>
              </div>
              <div class="setting-row-control w-url">
                <NInput
                  v-model:value="proxyStore.noProxy"
                  :placeholder="t('settings.general.proxyNoProxyPlaceholder')"
                  @blur="saveProxyNoProxy"
                  @keyup.enter="saveProxyNoProxy"
                />
              </div>
            </div>
          </template>
        </div>
        <p v-if="proxyError" class="settings-hint proxy-error">
          {{ proxyError }}
        </p>
        <p class="settings-hint">{{ t("settings.general.proxyDesc") }}</p>

        <div class="settings-subgroup">{{ t("settings.general.developerTitle") }}</div>
        <div class="settings-block">
          <div class="setting-row">
            <div class="setting-row-main">
              <div class="setting-row-label">{{ t("settings.general.devtools") }}</div>
              <div class="setting-row-desc">{{ t("settings.general.devtoolsDesc") }}</div>
            </div>
            <div class="setting-row-control">
              <NSwitch :value="devtoolsOn" @update:value="onDevtoolsChange" />
            </div>
          </div>
        </div>
      </template>
    </NForm>
  </section>
</template>

<style scoped>
/* 分组标题、页面骨架样式已抽到 src/assets/main.css 统一维护 */

.w-select {
  width: 240px;
}

.w-url {
  width: 320px;
}

.proxy-error {
  color: var(--ashell-danger, #e5484d);
}
</style>
