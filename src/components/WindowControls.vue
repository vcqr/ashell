<script setup lang="ts">
import {
  RiAddLine,
  RiCheckboxBlankLine,
  RiCloseLine,
  RiFileCopyLine,
  RiFullscreenExitLine,
  RiFullscreenLine,
  RiLayoutLeftLine,
  RiLayoutLine,
  RiLayoutRightLine,
  RiSubtractLine,
} from "@remixicon/vue"
import { computed, h, onBeforeUnmount, onMounted, ref } from "vue"
import { NDropdown, NIcon, type DropdownOption } from "naive-ui"
import { useI18n } from "vue-i18n"
import {
  currentMonitor,
  getCurrentWindow,
  PhysicalPosition,
  PhysicalSize,
} from "@tauri-apps/api/window"

import { detectMac } from "@/utils/platform"

/**
 * 无边框窗口的最小化/最大化/关闭按钮，Windows 风格与 macOS 红绿灯风格
 * 双样式自适配（detectMac）。自维护 isMaximized/isFullscreen（onResized 同步），
 * 可直接放进任意窗口的自绘标题栏；摆放位置（左/右）由父组件决定。
 * mac 绿灯：单击=全屏（系统默认）、Alt+单击=最大化，悬停弹出等效 Sequoia
 * 「Move & Resize」菜单（全屏/左右半屏平铺/最大化）。
 */
const { t } = useI18n()
const appWindow = getCurrentWindow()
const isMac = detectMac()

const isMaximized = ref(false)
const isFullscreen = ref(false)
let unlistenResize: (() => void) | null = null

async function syncWinState() {
  try {
    isMaximized.value = await appWindow.isMaximized()
    isFullscreen.value = await appWindow.isFullscreen()
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

async function toggleFullscreen() {
  try {
    await appWindow.setFullscreen(!isFullscreen.value)
    void syncWinState()
  } catch {
    // ignore
  }
}

/** 绿灯菜单「移到半屏」：按显示器工作区（去掉 Dock/菜单栏）平铺到左/右一半 */
async function tileWindow(side: "left" | "right") {
  try {
    if (isFullscreen.value) await appWindow.setFullscreen(false)
    const mon = await currentMonitor()
    if (!mon) return
    const wa = mon.workArea
    const half = Math.floor(wa.size.width / 2)
    const x = side === "left" ? wa.position.x : wa.position.x + half
    await appWindow.setPosition(new PhysicalPosition(x, wa.position.y))
    await appWindow.setSize(new PhysicalSize(half, wa.size.height))
  } catch {
    // ignore
  }
}

/** 绿灯：单击=全屏（macOS 系统默认行为），Alt+单击=最大化/还原 */
function onZoomClick(e: MouseEvent) {
  if (e.altKey) void toggleMaximize()
  else void toggleFullscreen()
}

/** 绿灯悬停菜单：等效 macOS Sequoia「Move & Resize」（该菜单属于 NSWindow
 * 标准按钮，自绘按钮无法触发，用 setPosition/setSize 复刻同等动作） */
const zoomOptions = computed<DropdownOption[]>(() => [
  {
    label: isFullscreen.value ? t("app.exitFullscreen") : t("app.fullscreen"),
    key: "fullscreen",
    icon: () =>
      h(NIcon, null, {
        default: () => h(isFullscreen.value ? RiFullscreenExitLine : RiFullscreenLine),
      }),
  },
  {
    label: t("app.tileLeft"),
    key: "left",
    icon: () => h(NIcon, null, { default: () => h(RiLayoutLeftLine) }),
  },
  {
    label: t("app.tileRight"),
    key: "right",
    icon: () => h(NIcon, null, { default: () => h(RiLayoutRightLine) }),
  },
  {
    label: isMaximized.value ? t("app.restore") : t("app.maximize"),
    key: "maximize",
    icon: () =>
      h(NIcon, null, {
        default: () => h(isMaximized.value ? RiFileCopyLine : RiLayoutLine),
      }),
  },
])

function onZoomSelect(key: string) {
  if (key === "fullscreen") void toggleFullscreen()
  else if (key === "left" || key === "right") void tileWindow(key)
  else void toggleMaximize()
}

function closeWindow() {
  appWindow.close()
}
</script>

<template>
  <div v-if="isMac" class="window-controls is-mac" data-tauri-drag-region="false">
    <button
      class="window-control mac-close"
      type="button"
      :title="t('app.closeWindow')"
      @click="closeWindow"
    >
      <NIcon :size="10">
        <RiCloseLine />
      </NIcon>
    </button>
    <button
      class="window-control mac-min"
      type="button"
      :title="t('app.minimize')"
      @click="minimizeWindow"
    >
      <NIcon :size="10">
        <RiSubtractLine />
      </NIcon>
    </button>
    <NDropdown
      trigger="hover"
      placement="bottom-start"
      :options="zoomOptions"
      @select="onZoomSelect"
    >
      <button
        class="window-control mac-max"
        type="button"
        :title="isFullscreen ? t('app.exitFullscreen') : t('app.fullscreen')"
        @click="onZoomClick"
      >
        <NIcon :size="10">
          <RiFileCopyLine v-if="isFullscreen" />
          <RiAddLine v-else />
        </NIcon>
      </button>
    </NDropdown>
  </div>
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

/* macOS traffic light style */
.window-controls.is-mac {
  margin-left: 4px;
  margin-right: 8px;
  gap: 8px;
  padding: 0 4px;
}

.window-controls.is-mac .window-control {
  width: 12px;
  height: 12px;
  min-width: 12px;
  border-radius: 50%;
  padding: 0;
  background: var(--ashell-mac-dot-idle, rgba(255, 255, 255, 0.16));
  color: rgba(0, 0, 0, 0.55);
  border: 0.5px solid rgba(0, 0, 0, 0.18);
  transition:
    background 0.15s ease,
    color 0.15s ease,
    transform 0.1s ease;
}

.window-controls.is-mac .window-control :deep(.n-icon) {
  opacity: 0;
  transition: opacity 0.12s ease;
}

.window-controls.is-mac:hover .window-control :deep(.n-icon) {
  opacity: 1;
}

.window-controls.is-mac .mac-close {
  background: #ff5f57;
}
.window-controls.is-mac .mac-min {
  background: #febc2e;
}
.window-controls.is-mac .mac-max {
  background: #28c840;
}

.window-controls.is-mac .window-control:hover {
  filter: brightness(1.05);
}

.window-controls.is-mac .window-control:active {
  filter: brightness(0.9);
  background: var(--ashell-mac-dot-active, currentColor);
}

.window-controls.is-mac .window-control-close:hover {
  background: #ff5f57;
  color: rgba(0, 0, 0, 0.6);
}
</style>
