<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue"
import { NIcon } from "naive-ui"
import { useI18n } from "vue-i18n"
import { getCurrentWindow } from "@tauri-apps/api/window"
import {
  AddOutline,
  CloseOutline,
  CopyOutline,
  RemoveOutline,
  SquareOutline,
} from "@vicons/ionicons5"
import { detectMac } from "@/utils/platform"

/**
 * 无边框窗口的最小化/最大化/关闭按钮，Windows 风格与 macOS 红绿灯风格
 * 双样式自适配（detectMac）。自维护 isMaximized（onResized 同步），
 * 可直接放进任意窗口的自绘标题栏；摆放位置（左/右）由父组件决定。
 */
const { t } = useI18n()
const appWindow = getCurrentWindow()
const isMac = detectMac()

const isMaximized = ref(false)
let unlistenResize: (() => void) | null = null

async function syncMaximized() {
  try {
    isMaximized.value = await appWindow.isMaximized()
  } catch {
    // ignore
  }
}

onMounted(() => {
  void syncMaximized()
  void appWindow
    .onResized(() => {
      void syncMaximized()
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
  void syncMaximized()
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
        <CloseOutline />
      </NIcon>
    </button>
    <button
      class="window-control mac-min"
      type="button"
      :title="t('app.minimize')"
      @click="minimizeWindow"
    >
      <NIcon :size="10">
        <RemoveOutline />
      </NIcon>
    </button>
    <button
      class="window-control mac-max"
      type="button"
      :title="isMaximized ? t('app.restore') : t('app.maximize')"
      @click="toggleMaximize"
    >
      <NIcon :size="10">
        <CopyOutline v-if="isMaximized" />
        <AddOutline v-else />
      </NIcon>
    </button>
  </div>
  <div v-else class="window-controls" data-tauri-drag-region="false">
    <button
      class="window-control"
      type="button"
      :title="t('app.minimize')"
      @click="minimizeWindow"
    >
      <NIcon :size="14">
        <RemoveOutline />
      </NIcon>
    </button>
    <button
      class="window-control"
      type="button"
      :title="isMaximized ? t('app.restore') : t('app.maximize')"
      @click="toggleMaximize"
    >
      <NIcon :size="13">
        <CopyOutline v-if="isMaximized" />
        <SquareOutline v-else />
      </NIcon>
    </button>
    <button
      class="window-control window-control-close"
      type="button"
      :title="t('app.closeWindow')"
      @click="closeWindow"
    >
      <NIcon :size="14">
        <CloseOutline />
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
