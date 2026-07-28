<script setup lang="ts">
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
import {
  FolderOpenOutline,
  RefreshOutline,
  ImagesOutline,
} from "@vicons/ionicons5";
import { invoke } from "@tauri-apps/api/core";
import { useIconStore } from "@/stores/icons";

const { t } = useI18n();
const iconStore = useIconStore();
const message = useMessage();

onMounted(() => {
  void iconStore.ensureLoaded();
});

async function openDir() {
  try {
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
    <div class="settings-hint">
      {{ t("settings.icons.hint") }}
    </div>

    <NSpace size="small">
      <NButton size="small" @click="openDir">
        <template #icon>
          <NIcon><FolderOpenOutline /></NIcon>
        </template>
        {{ t("settings.icons.openDir") }}
      </NButton>
      <NButton
        size="small"
        :loading="iconStore.loading"
        @click="refresh"
      >
        <template #icon>
          <NIcon><RefreshOutline /></NIcon>
        </template>
        {{ t("settings.icons.refresh") }}
      </NButton>
    </NSpace>

    <div class="icons-block">
      <NSpin v-if="iconStore.loading && iconStore.items.length === 0" />
      <NEmpty
        v-else-if="iconStore.items.length === 0"
        :description="t('settings.icons.empty')"
        size="small"
      >
        <template #icon>
          <NIcon><ImagesOutline /></NIcon>
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
  color: var(--ashell-text-subtle);
  font-size: 12px;
  line-height: 1.6;
}

.settings-hint code {
  background: var(--ashell-hover);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 11.5px;
}

.icons-block {
  margin-top: 8px;
  padding-top: 12px;
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
