<script setup lang="ts">
import { computed } from "vue"
import WindowControls from "@/components/WindowControls.vue"
import SftpDrawer from "@/components/SftpDrawer.vue"
import { detectMac } from "@/utils/platform"

/**
 * SFTP 独立窗口（由 SftpDrawer 的"在新窗口打开"弹出）。
 *
 * 复用同一 index.html，通过 URL query 接收 sid/主机信息；SFTP 会话仍在
 * 后端，原终端 tab 关闭/断开后本窗口的操作会失败。窗口为无边框自绘
 * chrome，样式与主窗口标题栏保持一致。
 *
 * 标题栏承担主机标题 + 窗口控制；刷新等操作按钮在 SftpDrawer 的路径
 * 栏里（standalone 模式不渲染面板头，避免双标题）。
 */
const launch = new URLSearchParams(window.location.search)
const sid = launch.get("sid")
const hostName = launch.get("title") ?? undefined
const hostAddr = launch.get("addr") ?? undefined

const isMac = detectMac()

const drawerTitle = computed(() => {
  const host = hostName ?? "SFTP"
  const addr = hostAddr?.trim()
  return addr ? `${host} (${addr})` : host
})

/** macOS：双击标题栏切换最大化（与主窗口 onHeaderDblClick 同策略） */
async function onHeaderDblClick(e: MouseEvent) {
  if (!isMac) return
  const target = e.target as HTMLElement | null
  if (target?.closest('[data-tauri-drag-region="false"]')) return
  e.stopPropagation()
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window")
    await getCurrentWindow().toggleMaximize()
  } catch {
    // ignore
  }
}
</script>

<template>
  <div class="sftpwin-root">
    <header class="sftpwin-header" data-tauri-drag-region @dblclick="onHeaderDblClick">
      <WindowControls v-if="isMac" />
      <div class="brand" data-tauri-drag-region>
        <div class="brand-logo">
          <img src="/icon.png" alt="AShell" />
        </div>
        <span class="brand-name">{{ drawerTitle }}</span>
      </div>
      <nav class="drag-spacer" data-tauri-drag-region />
      <WindowControls v-if="!isMac" />
    </header>
    <SftpDrawer
      standalone
      :open="true"
      :sid="sid"
      :host-name="hostName"
      :host-addr="hostAddr"
    />
  </div>
</template>

<style scoped>
.sftpwin-root {
  height: 100vh;
  width: 100vw;
  background: var(--ashell-bg);
  --ashell-activity-w: 0px;
}

.sftpwin-header {
  height: var(--ashell-header-h);
  display: flex;
  align-items: center;
  padding: 0 12px;
  gap: 8px;
  position: relative;
  z-index: 10;
  background: linear-gradient(
    180deg,
    var(--ashell-header-start) 0%,
    var(--ashell-header-end) 100%
  );
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
