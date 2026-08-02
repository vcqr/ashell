<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import {
  NDescriptions,
  NDescriptionsItem,
  NButton,
  NIcon,
  NText,
  NProgress,
  useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  LogoGithub,
  GitNetworkOutline,
  SyncOutline,
  CloudDownloadOutline,
} from "@vicons/ionicons5";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useUpdater } from "@/composables/useUpdater";

const { t } = useI18n();
const message = useMessage();

const GITHUB_URL = "https://github.com/vcqr/ashell";
const GITEE_URL = "https://gitee.com/vcqr/ashell";

const version = ref<string>("");

const {
  updateState,
  newVersion,
  releaseBody,
  downloadProgress,
  checkForUpdates,
  downloadAndInstall,
} = useUpdater();

const renderedReleaseNotes = computed(() => {
  if (!releaseBody.value) return "";
  try {
    const raw = marked.parse(releaseBody.value) as string;
    return DOMPurify.sanitize(raw);
  } catch {
    return "";
  }
});

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

async function handleCheck() {
  try {
    const found = await checkForUpdates();
    if (!found) {
      message.success(t("settings.about.upToDate"));
    }
  } catch (e) {
    message.error(t("settings.about.checkFailed", { error: String(e) }));
  }
}

async function handleDownload() {
  try {
    await downloadAndInstall(() =>
      message.success(t("settings.about.updateInstalled")),
    );
  } catch (e) {
    message.error(t("settings.about.downloadFailed", { error: String(e) }));
  }
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

    <div class="update-area">
      <NButton
        size="small"
        :loading="updateState === 'checking'"
        :disabled="updateState === 'downloading'"
        @click="handleCheck"
      >
        <template #icon>
          <NIcon><SyncOutline /></NIcon>
        </template>
        {{
          updateState === "checking"
            ? t("settings.about.checking")
            : t("settings.about.checkUpdate")
        }}
      </NButton>

      <template
        v-if="updateState === 'available' || updateState === 'downloading'"
      >
        <NText depth="2" class="update-version">
          {{ t("settings.about.newVersionAvailable", { version: newVersion }) }}
        </NText>
        <NButton
          v-if="updateState === 'available'"
          type="primary"
          size="small"
          @click="handleDownload"
        >
          <template #icon>
            <NIcon><CloudDownloadOutline /></NIcon>
          </template>
          {{ t("settings.about.downloadInstall") }}
        </NButton>
      </template>

      <NProgress
        v-if="updateState === 'downloading'"
        type="line"
        :percentage="downloadProgress"
        :height="6"
        :show-indicator="false"
        class="update-progress"
      />
    </div>

    <div
      v-if="
        (updateState === 'available' || updateState === 'downloading') &&
        renderedReleaseNotes
      "
      class="release-notes"
    >
      <div class="release-notes-title">
        {{ t("settings.about.releaseNotes") }}
      </div>
      <!-- eslint-disable-next-line vue/no-v-html -- DOMPurify 已对 marked 输出做消毒 -->
      <div class="release-notes-body" v-html="renderedReleaseNotes"></div>
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

.update-area {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  margin-top: 4px;
}

.update-version {
  font-size: 13px;
}

.update-progress {
  width: 100%;
}

.release-notes {
  max-height: 240px;
  overflow-y: auto;
  border: 1px solid var(--ashell-border-soft);
  border-radius: 8px;
  padding: 10px 14px;
}

.release-notes-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--ashell-text-muted);
  margin-bottom: 8px;
}

.release-notes-body {
  font-size: 13px;
  line-height: 1.6;
  color: var(--ashell-text);
}

.release-notes-body :deep(h1),
.release-notes-body :deep(h2),
.release-notes-body :deep(h3) {
  font-size: 14px;
  font-weight: 600;
  margin: 10px 0 6px;
}

.release-notes-body :deep(ul),
.release-notes-body :deep(ol) {
  padding-left: 20px;
  margin: 4px 0;
}

.release-notes-body :deep(li) {
  margin: 2px 0;
}

.release-notes-body :deep(code) {
  font-family: var(--ashell-mono-font, monospace);
  font-size: 12px;
  background: var(--ashell-hover);
  padding: 1px 4px;
  border-radius: 3px;
}

.release-notes-body :deep(pre) {
  background: var(--ashell-hover);
  padding: 8px 10px;
  border-radius: 6px;
  overflow-x: auto;
  margin: 6px 0;
}

.release-notes-body :deep(pre code) {
  background: none;
  padding: 0;
}

.release-notes-body :deep(a) {
  color: var(--n-color-primary, #18a058);
}

.release-notes-body :deep(p) {
  margin: 4px 0;
}
</style>
