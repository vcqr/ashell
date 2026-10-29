<script setup lang="ts">
import { RiRefreshLine } from "@remixicon/vue"
import { isTauri } from "@/utils/platform"
import { onTitlebarDblclick } from "@/utils/titlebar"
import { computed, ref } from "vue"
import { useI18n } from "vue-i18n"
import { NButton, NIcon } from "naive-ui"

import WindowControls from "@/components/WindowControls.vue"
import HostInfoDrawer from "@/components/HostInfoDrawer.vue"
import { detectMac } from "@/utils/platform"

/**
 * 主机信息独立窗口（由 HostInfoDrawer 的"在新窗口打开"弹出）。
 *
 * 复用同一 index.html，URL query 携带 sid/主机信息；SSH 会话按 sid 存活
 * 于后端 axum 进程，本窗口凭相同 sid 轮询 sysinfo，不重复建连。原终端
 * tab 关闭/断开后采集失败，面板内显示错误与重试按钮。
 *
 * 面板头在 standalone 下不渲染（避免双标题）：手动刷新上移到本窗口
 * 标题栏，关闭走系统窗口控制。
 */
const launch = new URLSearchParams(window.location.search)
const sid = launch.get("sid")
const hostName = launch.get("title") ?? undefined
const hostAddr = launch.get("addr") ?? undefined

const { t } = useI18n()
const isMac = detectMac()

const drawerRef = ref<InstanceType<typeof HostInfoDrawer> | null>(null)

const windowTitle = computed(() => {
  const host = hostName ?? t("hostInfo.title")
  const addr = hostAddr?.trim()
  return addr ? `${host} (${addr})` : host
})

function manualRefresh() {
  drawerRef.value?.manualRefresh()
}

/** 双击标题栏切换最大化：统一走 onTitlebarDblclick（内建原生路径已被守卫拦下） */
async function toggleMaximize() {
  if (!isTauri) return
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window")
    await getCurrentWindow().toggleMaximize()
  } catch {
    // ignore
  }
}
</script>

<template>
  <div class="hostinfowin-root">
    <!-- deep：整条子树可拖拽（可交互元素与 ="false" 区域自动排除），与主窗口头部同策略 -->
    <header
      class="hostinfowin-header"
      data-tauri-drag-region="deep"
      @dblclick="onTitlebarDblclick($event, toggleMaximize)"
    >
      <WindowControls v-if="isMac && isTauri" />
      <div class="brand">
        <div class="brand-logo">
          <img src="/icon.png" alt="AShell" />
        </div>
        <span class="brand-name">{{ windowTitle }}</span>
      </div>
      <nav class="drag-spacer" />
      <NButton
        v-if="sid"
        quaternary
        circle
        size="small"
        data-tauri-drag-region="false"
        :title="t('hostInfo.refresh')"
        @click="manualRefresh"
      >
        <template #icon>
          <NIcon><RiRefreshLine /></NIcon>
        </template>
      </NButton>
      <WindowControls v-if="!isMac && isTauri" />
    </header>
    <HostInfoDrawer
      ref="drawerRef"
      standalone
      :open="true"
      :sid="sid"
      :host-name="hostName"
      :host-info="hostAddr ? { addr: hostAddr, port: '', username: '' } : undefined"
    />
  </div>
</template>

<style scoped>
.hostinfowin-root {
  height: 100vh;
  width: 100vw;
  background: var(--ashell-bg);
  --ashell-activity-w: 0px;
}

.hostinfowin-header {
  height: var(--ashell-header-h);
  display: flex;
  align-items: center;
  padding: 0 12px;
  gap: 8px;
  position: relative;
  z-index: 10;
  background: var(--ashell-bg);
  border-bottom: 1px solid var(--ashell-border);
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 0 4px;
  min-width: 0;
}

.brand-logo {
  width: 24px;
  height: 24px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.brand-logo img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.brand-name {
  font-weight: 600;
  font-size: 14px;
  color: var(--ashell-text-strong);
  letter-spacing: 0.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.drag-spacer {
  flex: 1;
  height: 100%;
  min-width: 0;
  -webkit-app-region: no-drag;
}
</style>
