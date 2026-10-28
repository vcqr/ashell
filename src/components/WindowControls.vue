<script setup lang="ts">
import { RiCheckboxBlankLine, RiCloseLine, RiFileCopyLine, RiSubtractLine } from "@remixicon/vue"
import { onBeforeUnmount, onMounted, ref } from "vue"
import { NIcon } from "naive-ui"
import { useI18n } from "vue-i18n"
import { getCurrentWindow } from "@tauri-apps/api/window"

import { detectMac } from "@/utils/platform"

/**
 * 窗口最小化/最大化/关闭按钮（Windows/Linux 风格）；摆放位置（左/右）由
 * 父组件决定，自维护 isMaximized（onResized 同步）。
 * macOS 窗口经 lib.rs 转为原生 Titled+Overlay：红绿灯、圆角、阴影全部由
 * 系统绘制，本组件在 mac 上仅渲染占位容器（≈灯区宽度），防止头部内容
 * 压进灯区；绿钮悬停的平铺菜单由系统（Sequoia+）原生提供。
 */
const { t } = useI18n()
const appWindow = getCurrentWindow()
const isMac = detectMac()

const isMaximized = ref(false)
let unlistenResize: (() => void) | null = null

async function syncWinState() {
  try {
    isMaximized.value = await appWindow.isMaximized()
  } catch {
    // ignore
  }
}

onMounted(() => {
  void syncWinState()
  void appWindow
    .onResized(() => {
      void syncWinState()
    })
    .then((un) => {
      unlistenResize = un
    })
})

onBeforeUnmount(() => {
  unlistenResize?.()
  unlistenResize = null
})

function minimizeWindow() {
  appWindow.minimize()
}

async function toggleMaximize() {
  await appWindow.toggleMaximize()
  void syncWinState()
}

function closeWindow() {
  appWindow.close()
}
</script>

<template>
  <!-- macOS：原生红绿灯由系统绘制，这里仅占位（避免头部内容压进灯区）；
       容器本身是拖拽区，灯间空隙也能拖窗 -->
  <div v-if="isMac" class="window-controls is-mac" data-tauri-drag-region></div>
  <div v-else class="window-controls" data-tauri-drag-region="false">
    <button
      class="window-control"
      type="button"
      :title="t('app.minimize')"
      @click="minimizeWindow"
    >
      <NIcon :size="14">
        <RiSubtractLine />
      </NIcon>
    </button>
    <button
      class="window-control"
      type="button"
      :title="isMaximized ? t('app.restore') : t('app.maximize')"
      @click="toggleMaximize"
    >
      <NIcon :size="13">
        <RiFileCopyLine v-if="isMaximized" />
        <RiCheckboxBlankLine v-else />
      </NIcon>
    </button>
    <button
      class="window-control window-control-close"
      type="button"
      :title="t('app.closeWindow')"
      @click="closeWindow"
    >
      <NIcon :size="14">
        <RiCloseLine />
      </NIcon>
    </button>
  </div>
</template>

<style scoped>
.window-controls {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  margin-left: 8px;
  height: 100%;
  -webkit-app-region: no-drag;
}

.window-control {
  width: 40px;
  height: var(--ashell-header-h, 44px);
  border: 0;
  background: transparent;
  color: var(--ashell-text-muted);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 0;
  transition:
    background 0.12s ease,
    color 0.12s ease;
}

.window-control:hover {
  background: rgba(255, 255, 255, 0.08);
  color: var(--ashell-text-strong);
}

.window-control:active {
  background: rgba(255, 255, 255, 0.12);
}

.window-control-close:hover {
  background: #e81123;
  color: #fff;
}

/* macOS：原生红绿灯占位。灯区（约 7px 内距 + 3×12px 灯 + 2×8px 间距）≈60px，
 * 留出余量；覆盖基类的 no-drag，让占位区可拖拽窗口 */
.window-controls.is-mac {
  width: 68px;
  margin-left: 0;
  -webkit-app-region: drag;
}
</style>
