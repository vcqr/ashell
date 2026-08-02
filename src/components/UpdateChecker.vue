<script setup lang="ts">
import { onMounted, watch, h, ref } from "vue";
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

const activeNotification = ref<NotificationReactive | null>(null);

onMounted(() => {
  // 延迟 3s 检查，避免与启动初始化争抢资源
  window.setTimeout(() => void doStartupCheck(), 3000);
});

// 下载进度同步到通知 meta（setup 级 watch，随组件生命周期自动清理）
watch(downloadProgress, (p) => {
  if (updateState.value === "downloading" && activeNotification.value) {
    activeNotification.value.meta = `${t("settings.about.downloading")} ${p}%`;
  }
});

async function doStartupCheck() {
  // dev 模式下 updater 不可用，跳过自动检查
  if (import.meta.env.DEV) return;
  if (hasAutoChecked()) return;
  markAutoChecked();
  const found = await checkForUpdates({ silent: true });
  if (found) showUpdateNotification();
}

function showUpdateNotification() {
  activeNotification.value = notification.create({
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
          onClick: () => void doDownload(),
        },
        {
          default: () => t("settings.about.downloadInstall"),
          icon: () =>
            h(NIcon, null, { default: () => h(CloudDownloadOutline) }),
        },
      ),
  });
}

async function doDownload() {
  const n = activeNotification.value;
  if (!n) return;
  try {
    await downloadAndInstall(() => {
      n.title = `🔄 ${t("settings.about.updateInstalled")}`;
      n.content = "";
      n.meta = undefined;
    });
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
