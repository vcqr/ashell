<script setup lang="ts">
import { NSlider, NSwitch, NButton, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { pickImageFile } from "@/utils/fileInterop";
import { isTauri } from "@/utils/platform";
import { useTerminalStore } from "@/stores/terminal";

const { t } = useI18n();
const termStore = useTerminalStore();
const message = useMessage();

async function chooseWallpaper() {
  try {
    if (isTauri) {
      const { invoke } = await import("@tauri-apps/api/core");
      const path = await invoke<string | null>("pick_image_file");
      if (!path) return;
      await termStore.setWallpaper(path);
    } else {
      const file = await pickImageFile();
      if (!file) return;
      await termStore.uploadWallpaper(file);
    }
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
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.tabs.window") }}</div>
    <div class="settings-block">
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">{{ t("settings.window.opacity") }}</div>
        </div>
        <div class="setting-row-control w-slider">
          <NSlider
            :value="Math.round(termStore.windowOpacity * 100)"
            :min="30"
            :max="100"
            :step="1"
            :tooltip="true"
            :format-tooltip="(v: number) => `${v}%`"
            @update:value="(v: number) => termStore.setWindowOpacity(v / 100)"
          />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">{{ t("settings.window.blur") }}</div>
        </div>
        <div class="setting-row-control">
          <NSwitch
            :value="termStore.windowBlur"
            @update:value="(v: boolean) => termStore.setWindowBlur(v)"
          />
        </div>
      </div>
      <div v-if="termStore.windowBlur" class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">{{ t("settings.window.acrylicTint") }}</div>
        </div>
        <div class="setting-row-control w-slider">
          <NSlider
            :value="Math.round(termStore.acrylicTint * 100)"
            :min="0"
            :max="100"
            :step="1"
            :tooltip="true"
            :format-tooltip="(v: number) => `${v}%`"
            @update:value="(v: number) => termStore.setAcrylicTint(v / 100)"
          />
        </div>
      </div>
    </div>

    <div class="settings-block wp-block">
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">{{ t("settings.window.wallpaper") }}</div>
        </div>
        <div class="setting-row-control">
          <!-- 注意：这里不要用 NSpace 包 v-if 子项。naive-ui 的 Space 渲染时对每个
               子项包装 div 使用常量 key 并走 block 树补丁（STABLE_FRAGMENT），子节点
               数量因 v-if 变化时 diff 失准，会把同一份内容挂到所有旧占位上——表现为
               清除壁纸后出现 4 个“选择壁纸”按钮。改用普通 div + 作用域 CSS。 -->
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
        </div>
      </div>
      <!-- 预览 + 透明度不是“标签+控件”行式结构，保持原纵向布局放进卡片内留白容器 -->
      <div v-if="termStore.wallpaperUrl" class="wallpaper-extra">
        <div
          class="wallpaper-preview"
          :style="{
            backgroundImage: `url(${termStore.wallpaperUrl})`,
            opacity: termStore.wallpaperOpacity,
          }"
        />
        <div class="wallpaper-opacity-row">
          <span class="wallpaper-opacity-label">{{ t("settings.window.wallpaperOpacity") }}</span>
          <span class="wallpaper-opacity-value">{{ Math.round(termStore.wallpaperOpacity * 100) }}%</span>
        </div>
        <NSlider
          :value="Math.round(termStore.wallpaperOpacity * 100)"
          :min="0"
          :max="100"
          :step="1"
          :tooltip="true"
          :format-tooltip="(v: number) => `${v}%`"
          @update:value="(v: number) => termStore.setWallpaperOpacity(v / 100)"
        />
      </div>
    </div>

    <p class="settings-hint">{{ t("settings.window.hint") }}</p>
  </section>
</template>

<style scoped>
/* 分组标题、页面骨架样式已抽到 src/assets/main.css 统一维护 */

/* 行内滑杆限宽，避免横贯整张卡片 */
.w-slider {
  width: 220px;
}

/* 壁纸卡片：行式标题行之下留出呼吸感 */
.wp-block {
  padding-bottom: 0;
}

.wallpaper-extra {
  padding: 12px 0;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 12px;
}

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
