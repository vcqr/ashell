<script setup lang="ts">
import {
  RiFolderOpenLine,
  RiImageLine,
  RiRefreshLine,
} from "@remixicon/vue";
import { onMounted } from "vue";
import {
  NButton,
  NSpace,
  NIcon,
  NEmpty,
  NSpin,
  useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";

import { isTauri } from "@/utils/platform";
import { useIconStore } from "@/stores/icons";

const { t } = useI18n();
const iconStore = useIconStore();
const message = useMessage();

onMounted(() => {
  void iconStore.ensureLoaded();
});

async function openDir() {
  // 仅桌面形态提供（Web 端无"打开本机目录"能力，按钮已隐藏）
  if (!isTauri) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("open_icons_dir");
  } catch (e) {
    message.error(t("settings.icons.openDirFailed", { error: String(e) }));
  }
}

async function refresh() {
  try {
    await iconStore.refresh();
    message.success(t("settings.icons.refreshed", { count: iconStore.items.length }));
  } catch (e) {
    message.error(t("settings.icons.refreshFailed", { error: String(e) }));
  }
}
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.icons.title") }}</div>

    <div class="settings-block">
      <div class="icons-actions">
        <NSpace size="small">
          <NButton v-if="isTauri" size="small" @click="openDir">
            <template #icon>
              <NIcon><RiFolderOpenLine /></NIcon>
            </template>
            {{ t("settings.icons.openDir") }}
          </NButton>
          <NButton
            size="small"
            :loading="iconStore.loading"
            @click="refresh"
          >
            <template #icon>
              <NIcon><RiRefreshLine /></NIcon>
            </template>
            {{ t("settings.icons.refresh") }}
          </NButton>
        </NSpace>
      </div>

      <div class="icons-block">
        <NSpin v-if="iconStore.loading && iconStore.items.length === 0" />
        <NEmpty
          v-else-if="iconStore.items.length === 0"
          :description="t('settings.icons.empty')"
          size="small"
        >
          <template #icon>
            <NIcon><RiImageLine /></NIcon>
          </template>
        </NEmpty>
        <div v-else class="icons-grid">
          <div
            v-for="it in iconStore.items"
            :key="it.name"
            class="icon-item"
            :title="`${it.name} · ${it.size}B`"
          >
            <img
              class="icon-thumb"
              :src="iconStore.urlOf(it.name) ?? ''"
              :alt="it.name"
            />
            <span class="icon-name">{{ it.name }}</span>
          </div>
        </div>
      </div>
    </div>

    <div class="settings-hint">
      {{ t("settings.icons.hint") }}
    </div>
  </section>
</template>

<style scoped>
/* 分组标题、页面骨架样式已抽到 src/assets/main.css 统一维护 */

.settings-hint code {
  background: var(--ashell-hover);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 11.5px;
}

/* 卡片内非行式子块的留白容器：.settings-block 只给 4px 上下内边距 */
.icons-actions {
  padding: 10px 0 8px;
}

.icons-block {
  padding: 12px 0;
  border-top: 1px solid var(--ashell-border-soft);
  min-height: 80px;
}

.icons-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 12px;
}

.icon-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 10px 6px;
  border: 1px solid var(--ashell-border-soft);
  border-radius: 8px;
  background: var(--ashell-hover);
  min-width: 0;
}

.icon-thumb {
  width: 32px;
  height: 32px;
  object-fit: contain;
  flex-shrink: 0;
}

.icon-name {
  font-size: 11.5px;
  color: var(--ashell-text-muted);
  max-width: 100%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
