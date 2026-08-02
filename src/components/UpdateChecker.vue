<script setup lang="ts">
import { onMounted, watch, h } from "vue";
import {
  useNotification,
  NButton,
  NIcon,
  type NotificationReactive,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { CloudDownloadOutline } from "@vicons/ionicons5";
import { useUpdater } from "@/composables/useUpdater";

const { t } = useI18n();
const notification = useNotification();
const {
  updateState,
  newVersion,
  downloadProgress,
  checkForUpdates,
  downloadAndInstall,
  markAutoChecked,
  hasAutoChecked,
} = useUpdater();

onMounted(() => {
  // 延迟 3s 检查，避免与启动初始化争抢资源
  window.setTimeout(() => void doStartupCheck(), 3000);
});

async function doStartupCheck() {
  if (hasAutoChecked()) return;
  markAutoChecked();
  const found = await checkForUpdates({ silent: true });
  if (found) showUpdateNotification();
}

function showUpdateNotification() {
  const n = notification.create({
    title: `🚀 ${t("settings.about.newVersionAvailable", {
      version: newVersion.value,
    })}`,
    content: t("settings.about.updateNotificationDesc"),
    type: "info",
    duration: 0,
    action: () =>
      h(
        NButton,
        {
          type: "primary",
          size: "small",
          loading: updateState.value === "downloading",
          disabled: updateState.value === "downloading",
          onClick: () => void doDownload(n),
        },
        {
          default: () => t("settings.about.downloadInstall"),
          icon: () =>
            h(NIcon, null, { default: () => h(CloudDownloadOutline) }),
        },
      ),
  });

  // 下载进度同步到通知 meta
  watch(downloadProgress, (p) => {
    if (updateState.value === "downloading") {
      n.meta = `${t("settings.about.downloading")} ${p}%`;
    }
  });
}

async function doDownload(n: NotificationReactive) {
  try {
    await downloadAndInstall();
    // downloadAndInstall 成功后会自动 relaunch，不会执行到这里
  } catch (e) {
    n.type = "error";
    n.title = t("settings.about.downloadFailed", { error: String(e) });
    n.meta = undefined;
    n.action = () =>
      h(
        NButton,
        { size: "small", onClick: () => n.destroy() },
        { default: () => t("common.close") },
      );
  }
}
</script>

<template></template>
