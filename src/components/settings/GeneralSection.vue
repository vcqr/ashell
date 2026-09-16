<script setup lang="ts">
import { ref } from "vue";
import {
  NForm,
  NFormItem,
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

    <div v-if="isTauri" class="dev-block">
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

.dev-block {
  margin-top: 14px;
}

.settings-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.6;
}
</style>
