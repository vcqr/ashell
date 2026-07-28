<script setup lang="ts">
import {
  NForm,
  NFormItem,
  NSelect,
  type SelectOption,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  localePreference,
  setLocalePreference,
  type LocalePreference,
} from "@/locales";

const { t } = useI18n();

const languageOptions: SelectOption[] = [
  { label: "简体中文", value: "zh-CN" },
  { label: "English", value: "en-US" },
  { label: "Auto (Follow system)", value: "auto" },
];

function onChange(value: LocalePreference) {
  setLocalePreference(value);
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
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.6;
}
</style>
