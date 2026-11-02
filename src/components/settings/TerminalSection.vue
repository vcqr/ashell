<script setup lang="ts">
import { computed } from "vue";
import {
  NForm,
  NSelect,
  NInput,
  NInputNumber,
  NRadioGroup,
  NRadioButton,
  NSwitch,
  NSpace,
  NButton,
  NColorPicker,
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
  CURSOR_WIDTH_MIN,
  CURSOR_WIDTH_MAX,
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

// "断开自动关闭"与"断线自动重连"互斥：标签都要关了，重连没有意义
const disconnectPolicyActive = computed(
  () => terminalStore.disconnectAction !== "keep",
);

/** 行时间戳常用格式预设：选中即填入自定义输入框；当前值不匹配任何预设时显示占位 */
const timestampFormatPresets = computed(() => [
  { label: "HH:mm:ss", value: "HH:mm:ss" },
  { label: "MM-DD HH:mm:ss", value: "MM-DD HH:mm:ss" },
  { label: "YYYY-MM-DD HH:mm:ss", value: "YYYY-MM-DD HH:mm:ss" },
  { label: "YYYY-MM-DD HH:mm:ss.SSS", value: "YYYY-MM-DD HH:mm:ss.SSS" },
]);

const timestampPreset = computed({
  get: () =>
    timestampFormatPresets.value.find((p) => p.value === terminalStore.lineTimestampFormat)
      ?.value ?? null,
  set: (v: string | null) => {
    if (v) terminalStore.lineTimestampFormat = v;
  },
});

function onDisconnectActionChange(value: DisconnectAction) {
  terminalStore.disconnectAction = value;
  if (value !== "keep" && terminalStore.autoReconnect) {
    terminalStore.autoReconnect = false;
  }
}
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.terminal.title") }}</div>
    <NForm size="small" :show-feedback="false">
      <div class="settings-subgroup">{{ t("settings.terminal.groupFont") }}</div>
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.fontFamily") }}</div>
          </div>
          <div class="setting-row-control w-font">
            <NSelect
              v-model:value="terminalStore.fontFamily"
              :options="fontFamilyOptions"
              :loading="terminalStore.systemFontsLoading"
              filterable
              tag
              :placeholder="t('settings.terminal.fontFamilyPlaceholder')"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.fontSize") }}</div>
          </div>
          <div class="setting-row-control w-num">
            <NInputNumber
              v-model:value="terminalStore.fontSize"
              :min="FONT_SIZE_MIN"
              :max="FONT_SIZE_MAX"
              :step="1"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.scrollback") }}</div>
          </div>
          <div class="setting-row-control w-num">
            <NInputNumber
              v-model:value="terminalStore.scrollback"
              :min="SCROLLBACK_MIN"
              :max="SCROLLBACK_MAX"
              :step="1000"
            />
          </div>
        </div>
      </div>

      <div class="settings-subgroup">{{ t("settings.terminal.groupCursor") }}</div>
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.cursorStyle") }}</div>
          </div>
          <div class="setting-row-control">
            <NRadioGroup v-model:value="terminalStore.cursorStyle">
              <NRadioButton
                v-for="opt in cursorStyleOptions"
                :key="opt.value"
                :value="opt.value"
              >
                {{ opt.label }}
              </NRadioButton>
            </NRadioGroup>
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.cursorBlink") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.cursorBlink" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.cursorWidth") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.cursorWidthHint") }}</div>
          </div>
          <div class="setting-row-control w-num">
            <NInputNumber
              v-model:value="terminalStore.cursorWidth"
              :min="CURSOR_WIDTH_MIN"
              :max="CURSOR_WIDTH_MAX"
              :step="1"
            />
          </div>
        </div>
      </div>

      <div class="settings-subgroup">{{ t("settings.terminal.groupMouse") }}</div>
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.rightClick") }}</div>
          </div>
          <div class="setting-row-control w-action">
            <NSelect
              v-model:value="terminalStore.rightClickAction"
              :options="rightClickOptions"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.leftClick") }}</div>
          </div>
          <div class="setting-row-control w-action">
            <NSelect
              v-model:value="terminalStore.leftClickAction"
              :options="leftClickOptions"
            />
          </div>
        </div>
      </div>

      <div class="settings-subgroup">{{ t("settings.terminal.groupConnection") }}</div>
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.disconnectAction") }}</div>
          </div>
          <div class="setting-row-control w-disconnect">
            <NSelect
              :value="terminalStore.disconnectAction"
              :options="disconnectOptions"
              @update:value="onDisconnectActionChange"
            />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.autoReconnect") }}</div>
            <div class="setting-row-desc">
              {{
                disconnectPolicyActive
                  ? t("settings.terminal.autoReconnectDisabledHint")
                  : t("settings.terminal.autoReconnectHint")
              }}
            </div>
          </div>
          <div class="setting-row-control">
            <NSwitch
              v-model:value="terminalStore.autoReconnect"
              :disabled="disconnectPolicyActive"
            />
          </div>
        </div>
      </div>

      <div class="settings-subgroup">{{ t("settings.terminal.groupShortcuts") }}</div>
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.tabShortcuts") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.tabShortcutsHint") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.tabShortcutsEnabled" />
          </div>
        </div>
      </div>

      <div class="settings-subgroup">{{ t("settings.terminal.rendering") }}</div>
      <div class="settings-block">
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.webgl") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.webglDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.webglEnabled" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.webLinks") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.webLinksDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.webLinksEnabled" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.unicode11") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.unicode11Desc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.unicode11Enabled" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.searchHotkey") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.searchHotkeyDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.searchHotkeyEnabled" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.ligatures") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.ligaturesDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.ligaturesEnabled" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.progress") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.progressDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.progressEnabled" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.commandSuggest") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.commandSuggestDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NPopconfirm @positive-click="clearCommandSuggestData">
              <template #trigger>
                <NButton size="small" secondary>
                  {{ t("settings.terminal.clearHistory") }}
                </NButton>
              </template>
              {{ t("settings.terminal.clearHistoryConfirm") }}
            </NPopconfirm>
            <NSwitch v-model:value="terminalStore.commandSuggestEnabled" />
          </div>
        </div>
        <div class="setting-row">
          <div class="setting-row-main">
            <div class="setting-row-label">{{ t("settings.terminal.lineTimestamp") }}</div>
            <div class="setting-row-desc">{{ t("settings.terminal.lineTimestampDesc") }}</div>
          </div>
          <div class="setting-row-control">
            <NSwitch v-model:value="terminalStore.lineTimestampEnabled" />
          </div>
        </div>
      </div>
      <template v-if="terminalStore.lineTimestampEnabled">
        <div class="settings-block ts-sub">
          <div class="setting-row">
            <div class="setting-row-main">
              <div class="setting-row-label">{{ t("settings.terminal.lineTimestampFormat") }}</div>
              <div class="setting-row-desc">{{ t("settings.terminal.lineTimestampFormatHint") }}</div>
            </div>
            <div class="setting-row-control ts-format">
              <NSelect
                v-model:value="timestampPreset"
                :options="timestampFormatPresets"
                :placeholder="t('settings.terminal.lineTimestampPresetPlaceholder')"
              />
              <NInput
                v-model:value="terminalStore.lineTimestampFormat"
                :placeholder="t('settings.terminal.lineTimestampFormatPlaceholder')"
                :maxlength="64"
              />
            </div>
          </div>
          <div class="setting-row">
            <div class="setting-row-main">
              <div class="setting-row-label">{{ t("settings.terminal.lineTimestampColor") }}</div>
            </div>
            <div class="setting-row-control ts-color">
              <NColorPicker
                v-model:value="terminalStore.lineTimestampColor"
                :show-alpha="false"
                :modes="['hex']"
              />
            </div>
          </div>
        </div>
      </template>
    </NForm>
    <NSpace style="margin-top: 16px">
      <NPopconfirm @positive-click="terminalStore.resetDefaults()">
        <template #trigger>
          <NButton size="small">
            {{ t("settings.terminal.resetDefaults") }}
          </NButton>
        </template>
        {{ t("settings.terminal.resetDefaultsConfirm") }}
      </NPopconfirm>
    </NSpace>
  </section>
</template>

<style scoped>
/* 分组标题、页面骨架样式已抽到 src/assets/main.css 统一维护 */

.w-font {
  width: 320px;
}

.w-num {
  width: 140px;
}

.w-action {
  width: 280px;
}

.w-disconnect {
  width: 320px;
}

/* 行时间戳展开的从属子项：紧跟主卡片之下的独立卡片 */
.ts-sub {
  margin-top: 10px;
}

/* 预设下拉 + 自定义输入并排；颜色块用容器限宽（NColorPicker 上的
   width 内联样式落不到根元素，见会话记录） */
.ts-format {
  width: 420px;
}
.ts-format > :first-child {
  width: 170px;
  flex-shrink: 0;
}
.ts-format > :last-child {
  flex: 1;
  min-width: 0;
}

.ts-color {
  width: 140px;
}
</style>
