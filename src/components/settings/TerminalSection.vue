<script setup lang="ts">
import { computed } from "vue";
import {
  NForm,
  NFormItem,
  NSelect,
  NInputNumber,
  NRadioGroup,
  NRadioButton,
  NSwitch,
  NSpace,
  NButton,
  NPopconfirm,
  type SelectOption,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { clearCommandSuggestData } from "@/composables/useCommandSuggest";
import {
  useTerminalStore,
  FONT_FAMILY_PRESETS,
  FONT_SIZE_MIN,
  FONT_SIZE_MAX,
  SCROLLBACK_MIN,
  SCROLLBACK_MAX,
  type CursorStyle,
  type RightClickAction,
  type LeftClickAction,
  type DisconnectAction,
} from "@/stores/terminal";

const { t } = useI18n();
const terminalStore = useTerminalStore();

const fontFamilyOptions = computed<SelectOption[]>(() => {
  const presetGroup: SelectOption = {
    type: "group",
    label: t("settings.terminal.presetGroup"),
    key: "preset",
    children: FONT_FAMILY_PRESETS.map((p) => ({
      label: p.label,
      value: p.value,
    })),
  };
  const systemFonts = terminalStore.systemFonts;
  if (!systemFonts || systemFonts.length === 0) {
    return [presetGroup];
  }
  const systemGroup: SelectOption = {
    type: "group",
    label: t("settings.terminal.systemGroup"),
    key: "system",
    children: systemFonts.map((name) => ({
      label: name,
      value: `'${name}', monospace`,
    })),
  };
  return [presetGroup, systemGroup];
});

const cursorStyleOptions = computed(() => [
  { label: t("settings.terminal.cursorStyles.block"), value: "block" as CursorStyle },
  { label: t("settings.terminal.cursorStyles.underline"), value: "underline" as CursorStyle },
  { label: t("settings.terminal.cursorStyles.bar"), value: "bar" as CursorStyle },
]);

const rightClickOptions = computed(() => [
  { label: t("settings.terminal.rightClickActions.paste"), value: "paste" as RightClickAction },
  { label: t("settings.terminal.rightClickActions.smart"), value: "smart" as RightClickAction },
  { label: t("settings.terminal.rightClickActions.contextMenu"), value: "contextMenu" as RightClickAction },
  { label: t("settings.terminal.rightClickActions.none"), value: "none" as RightClickAction },
]);

const leftClickOptions = computed(() => [
  { label: t("settings.terminal.leftClickActions.copyOnSelect"), value: "copyOnSelect" as LeftClickAction },
  { label: t("settings.terminal.leftClickActions.copyAndMiddlePaste"), value: "copyAndMiddlePaste" as LeftClickAction },
  { label: t("settings.terminal.leftClickActions.middlePasteOnly"), value: "middlePasteOnly" as LeftClickAction },
  { label: t("settings.terminal.leftClickActions.none"), value: "none" as LeftClickAction },
]);

const disconnectOptions = computed(() => [
  { label: t("settings.terminal.disconnectActions.keep"), value: "keep" as DisconnectAction },
  { label: t("settings.terminal.disconnectActions.closeTab"), value: "closeTab" as DisconnectAction },
  { label: t("settings.terminal.disconnectActions.closeWindow"), value: "closeWindow" as DisconnectAction },
]);
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.terminal.title") }}</div>
    <NForm
      label-placement="top"
      require-mark-placement="right-hanging"
      size="small"
      :show-feedback="false"
    >
      <div class="settings-subgroup">{{ t("settings.terminal.groupFont") }}</div>
      <NFormItem :label="t('settings.terminal.fontFamily')">
        <NSelect
          v-model:value="terminalStore.fontFamily"
          :options="fontFamilyOptions"
          :loading="terminalStore.systemFontsLoading"
          filterable
          tag
          :placeholder="t('settings.terminal.fontFamilyPlaceholder')"
        />
      </NFormItem>
      <div class="form-row" style="margin-top: 12px">
        <NFormItem :label="t('settings.terminal.fontSize')" style="flex: 1">
          <NInputNumber
            v-model:value="terminalStore.fontSize"
            :min="FONT_SIZE_MIN"
            :max="FONT_SIZE_MAX"
            :step="1"
            style="width: 100%"
          />
        </NFormItem>
        <NFormItem :label="t('settings.terminal.scrollback')" style="flex: 1">
          <NInputNumber
            v-model:value="terminalStore.scrollback"
            :min="SCROLLBACK_MIN"
            :max="SCROLLBACK_MAX"
            :step="1000"
            style="width: 100%"
          />
        </NFormItem>
      </div>

      <div class="settings-subgroup" style="margin-top: 16px">{{ t("settings.terminal.groupCursor") }}</div>
      <div class="form-row" style="margin-top: 12px">
        <NFormItem :label="t('settings.terminal.cursorStyle')" style="flex: 1">
          <NRadioGroup v-model:value="terminalStore.cursorStyle">
            <NRadioButton
              v-for="opt in cursorStyleOptions"
              :key="opt.value"
              :value="opt.value"
            >
              {{ opt.label }}
            </NRadioButton>
          </NRadioGroup>
        </NFormItem>
        <NFormItem :label="t('settings.terminal.cursorBlink')">
          <NSwitch v-model:value="terminalStore.cursorBlink" />
        </NFormItem>
      </div>

      <div class="settings-subgroup" style="margin-top: 16px">{{ t("settings.terminal.groupMouse") }}</div>
      <div class="form-row" style="margin-top: 12px">
        <NFormItem :label="t('settings.terminal.rightClick')" style="flex: 1">
          <NSelect
            v-model:value="terminalStore.rightClickAction"
            :options="rightClickOptions"
          />
        </NFormItem>
        <NFormItem :label="t('settings.terminal.leftClick')" style="flex: 1">
          <NSelect
            v-model:value="terminalStore.leftClickAction"
            :options="leftClickOptions"
          />
        </NFormItem>
      </div>

      <div class="settings-subgroup" style="margin-top: 16px">{{ t("settings.terminal.groupConnection") }}</div>
      <NFormItem :label="t('settings.terminal.disconnectAction')" style="margin-top: 12px">
        <NSelect
          v-model:value="terminalStore.disconnectAction"
          :options="disconnectOptions"
        />
      </NFormItem>
      <NFormItem :label="t('settings.terminal.autoReconnect')" style="margin-top: 12px">
        <NSwitch v-model:value="terminalStore.autoReconnect" />
      </NFormItem>
      <p class="settings-hint">{{ t("settings.terminal.autoReconnectHint") }}</p>

      <div class="settings-subgroup" style="margin-top: 16px">{{ t("settings.terminal.groupShortcuts") }}</div>
      <NFormItem :label="t('settings.terminal.tabShortcuts')" style="margin-top: 12px">
        <NSwitch v-model:value="terminalStore.tabShortcutsEnabled" />
      </NFormItem>
      <p class="settings-hint">{{ t("settings.terminal.tabShortcutsHint") }}</p>
    </NForm>
    <div class="settings-section-title" style="margin-top: 16px">{{ t("settings.terminal.rendering") }}</div>
    <NForm
      label-placement="top"
      require-mark-placement="right-hanging"
      size="small"
      :show-feedback="false"
    >
      <NFormItem :label="t('settings.terminal.webgl')">
        <NSwitch v-model:value="terminalStore.webglEnabled" />
      </NFormItem>
      <NFormItem :label="t('settings.terminal.webLinks')" style="margin-top: 12px">
        <NSwitch v-model:value="terminalStore.webLinksEnabled" />
      </NFormItem>
      <NFormItem :label="t('settings.terminal.unicode11')" style="margin-top: 12px">
        <NSwitch v-model:value="terminalStore.unicode11Enabled" />
      </NFormItem>
      <NFormItem :label="t('settings.terminal.searchHotkey')" style="margin-top: 12px">
        <NSwitch v-model:value="terminalStore.searchHotkeyEnabled" />
      </NFormItem>
      <NFormItem :label="t('settings.terminal.ligatures')" style="margin-top: 12px">
        <NSwitch v-model:value="terminalStore.ligaturesEnabled" />
      </NFormItem>
      <NFormItem :label="t('settings.terminal.progress')" style="margin-top: 12px">
        <NSwitch v-model:value="terminalStore.progressEnabled" />
      </NFormItem>
      <NFormItem :label="t('settings.terminal.commandSuggest')" style="margin-top: 12px">
        <NSpace align="center">
          <NSwitch v-model:value="terminalStore.commandSuggestEnabled" />
          <NPopconfirm @positive-click="clearCommandSuggestData">
            <template #trigger>
              <NButton size="small" secondary>
                {{ t("settings.terminal.clearHistory") }}
              </NButton>
            </template>
            {{ t("settings.terminal.clearHistoryConfirm") }}
          </NPopconfirm>
        </NSpace>
      </NFormItem>
    </NForm>
    <NSpace style="margin-top: 16px">
      <NButton size="small" @click="terminalStore.resetDefaults()">
        {{ t("settings.terminal.resetDefaults") }}
      </NButton>
    </NSpace>
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

.settings-subgroup {
  font-size: 12px;
  font-weight: 500;
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.4));
}

.settings-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.6;
}

.form-row {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}
</style>
