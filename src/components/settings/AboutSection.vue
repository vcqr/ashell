<script setup lang="ts">
import { ref, onMounted } from "vue";
import { NDescriptions, NDescriptionsItem, NButton, NIcon } from "naive-ui";
import { useI18n } from "vue-i18n";
import { LogoGithub, GitNetworkOutline } from "@vicons/ionicons5";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

const { t } = useI18n();

const GITHUB_URL = "https://github.com/vcqr/ashell";
const GITEE_URL = "https://gitee.com/vcqr/ashell";

const version = ref<string>("");

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    version.value = t("common.unknown");
  }
});

function open(url: string) {
  openUrl(url).catch((err) => console.error(t("common.openLinkFailed"), err));
}
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.about.title") }}</div>
    <div class="about-header">
      <img src="/icon.png" alt="AShell" class="about-icon" />
      <p class="intro">
        {{ t("settings.about.intro") }}
      </p>
    </div>
    <NDescriptions :column="1" size="small" label-placement="left">
      <NDescriptionsItem :label="t('settings.about.appName')">AShell</NDescriptionsItem>
      <NDescriptionsItem :label="t('settings.about.version')">{{
        version || t("settings.about.loadingVersion")
      }}</NDescriptionsItem>
      <NDescriptionsItem :label="t('settings.about.github')">
        <NButton text type="primary" @click="open(GITHUB_URL)">
          <template #icon>
            <NIcon><LogoGithub /></NIcon>
          </template>
          {{ GITHUB_URL }}
        </NButton>
      </NDescriptionsItem>
      <NDescriptionsItem :label="t('settings.about.gitee')">
        <NButton text type="primary" @click="open(GITEE_URL)">
          <template #icon>
            <NIcon><GitNetworkOutline /></NIcon>
          </template>
          {{ GITEE_URL }}
        </NButton>
      </NDescriptionsItem>
    </NDescriptions>
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

.intro {
  margin: 0;
  font-size: 13px;
  line-height: 1.7;
  color: var(--ashell-text);
}

.about-header {
  display: flex;
  align-items: flex-start;
  gap: 14px;
}

.about-icon {
  width: 64px;
  height: 64px;
  border-radius: 14px;
  flex-shrink: 0;
  object-fit: contain;
}

.settings-section :deep(.n-descriptions),
.settings-section :deep(.n-descriptions .n-descriptions-table-header),
.settings-section :deep(.n-descriptions .n-descriptions-table-content) {
  font-size: 13px;
}

.settings-section :deep(.n-descriptions .n-button) {
  font-size: 13px;
}
</style>
