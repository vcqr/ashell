import { ref } from "vue";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "error";

// 模块级单例状态 -- UpdateChecker（启动检查）与 AboutSection（手动检查）共享
const updateState = ref<UpdateState>("idle");
const pendingUpdate = ref<Update | null>(null);
const newVersion = ref("");
const releaseBody = ref("");
const downloadProgress = ref(0);
let autoChecked = false;

export function useUpdater() {
  /**
   * 检查更新。silent=true 时静默吞掉错误（用于启动自动检查）。
   * 返回是否发现新版本。
   */
  async function checkForUpdates(opts?: {
    silent?: boolean;
  }): Promise<boolean> {
    const silent = opts?.silent ?? false;
    updateState.value = "checking";
    pendingUpdate.value = null;
    try {
      const update = await check();
      if (update) {
        pendingUpdate.value = update;
        newVersion.value = update.version;
        releaseBody.value = update.body ?? "";
        updateState.value = "available";
        return true;
      }
      updateState.value = "idle";
      return false;
    } catch (e) {
      updateState.value = "idle";
      if (!silent) throw e;
      return false;
    }
  }

  /**
   * 下载并安装更新，安装完成后自动重启。
   * onBeforeRelaunch 在安装完成、重启前调用，用于展示"正在重启"提示。
   */
  async function downloadAndInstall(onBeforeRelaunch?: () => void): Promise<void> {
    const update = pendingUpdate.value;
    if (!update) return;
    updateState.value = "downloading";
    downloadProgress.value = 0;
    try {
      let contentLength = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            if (contentLength > 0) {
              downloadProgress.value = Math.min(
                100,
                Math.round((downloaded / contentLength) * 100),
              );
            }
            break;
          case "Finished":
            downloadProgress.value = 100;
            break;
        }
      });
      onBeforeRelaunch?.();
      // 给 UI 一点时间渲染"正在重启"提示
      await new Promise((r) => setTimeout(r, 800));
      await relaunch();
    } catch (e) {
      updateState.value = "available";
      throw e;
    }
  }

  function markAutoChecked() {
    autoChecked = true;
  }

  function hasAutoChecked() {
    return autoChecked;
  }

  return {
    updateState,
    pendingUpdate,
    newVersion,
    releaseBody,
    downloadProgress,
    checkForUpdates,
    downloadAndInstall,
    markAutoChecked,
    hasAutoChecked,
  };
}
