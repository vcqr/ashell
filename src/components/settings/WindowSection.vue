<script setup lang="ts">
import { NForm, NFormItem, NSlider, NSwitch, NText, NButton, NSpace, NDivider, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import { useTerminalStore } from "@/stores/terminal";

const { t } = useI18n();
const termStore = useTerminalStore();
const message = useMessage();

async function chooseWallpaper() {
  try {
    const path = await invoke<string | null>("pick_image_file");
    if (!path) return;
    await termStore.setWallpaper(path);
    message.success(t("settings.window.wallpaperSet"));
  } catch (e) {
    message.error(t("settings.window.wallpaperSetFailed", { error: String(e) }));
  }
}

async function removeWallpaper() {
  try {
    await termStore.clearWallpaper();
    message.success(t("settings.window.wallpaperCleared"));
  } catch (e) {
    message.error(t("settings.window.wallpaperClearFailed", { error: String(e) }));
  }
}
</script>

<template>
  <NForm label-placement="top">
    <NFormItem :label="t('settings.window.opacity')">
      <NSlider
        :value="Math.round(termStore.windowOpacity * 100)"
        :min="30"
        :max="100"
        :step="1"
        :tooltip="true"
        :format-tooltip="(v: number) => `${v}%`"
        @update:value="(v: number) => termStore.setWindowOpacity(v / 100)"
      />
    </NFormItem>
    <NFormItem :label="t('settings.window.blur')">
      <NSwitch
        :value="termStore.windowBlur"
        @update:value="(v: boolean) => termStore.setWindowBlur(v)"
      />
    </NFormItem>

    <NDivider style="margin: 16px 0 12px" />

    <NFormItem :label="t('settings.window.wallpaper')">
      <NSpace vertical :size="12" style="width: 100%">
        <div
          v-if="termStore.wallpaperUrl"
          class="wallpaper-preview"
          :style="{
            backgroundImage: `url(${termStore.wallpaperUrl})`,
            opacity: termStore.wallpaperOpacity,
          }"
        />
        <NSpace :size="8">
          <NButton size="small" @click="chooseWallpaper">
            {{ termStore.wallpaperUrl ? t("settings.window.changeWallpaper") : t("settings.window.selectWallpaper") }}
          </NButton>
          <NButton
            v-if="termStore.wallpaperUrl"
            size="small"
            quaternary
            type="error"
            @click="removeWallpaper"
          >
            {{ t("settings.window.clear") }}
          </NButton>
        </NSpace>
        <div v-if="termStore.wallpaperUrl" class="wallpaper-opacity-row">
          <span class="wallpaper-opacity-label">{{ t("settings.window.wallpaperOpacity") }}</span>
          <span class="wallpaper-opacity-value">{{ Math.round(termStore.wallpaperOpacity * 100) }}%</span>
        </div>
        <NSlider
          v-if="termStore.wallpaperUrl"
          :value="Math.round(termStore.wallpaperOpacity * 100)"
          :min="0"
          :max="100"
          :step="1"
          :tooltip="true"
          :format-tooltip="(v: number) => `${v}%`"
          @update:value="(v: number) => termStore.setWallpaperOpacity(v / 100)"
        />
      </NSpace>
    </NFormItem>

    <NText depth="3" style="font-size: 12px; line-height: 1.6">
      {{ t("settings.window.hint") }}
    </NText>
  </NForm>
</template>

<style scoped>
.wallpaper-preview {
  width: 100%;
  height: 120px;
  border-radius: 8px;
  background-size: cover;
  background-position: center;
  background-repeat: no-repeat;
  border: 1px solid var(--ashell-border);
}

.wallpaper-opacity-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  font-size: 13px;
}

.wallpaper-opacity-label {
  color: var(--ashell-text-muted);
}

.wallpaper-opacity-value {
  color: var(--ashell-text-subtle);
  font-variant-numeric: tabular-nums;
}
</style>
