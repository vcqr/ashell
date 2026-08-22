<script setup lang="ts">
import { computed, ref } from "vue"
import { useI18n } from "vue-i18n"
import { NButton, NIcon } from "naive-ui"
import { RefreshOutline } from "@vicons/ionicons5"
import WindowControls from "@/components/WindowControls.vue"
import AiAssistant from "@/components/AiAssistant.vue"
import { detectMac } from "@/utils/platform"

/**
 * AI 独立窗口（由 AiAssistant 面板的"在新窗口打开"弹出）。
 *
 * 复用同一 index.html，URL query 携带 ssid；sidecar 进程仍按 ssid 存活于
 * 后端，本窗口经 has_sidecar 附着监听同一事件流，不重复拉起进程。对话
 * 历史在弹出时经 localStorage 移交，由 AiAssistant(standalone) 取回。
 * 原终端 tab 关闭后 sidecar 被回收，本窗口继续发送会失败（与 SFTP 独立
 * 窗口同样的生命周期约束）。
 *
 * 面板头在 standalone 下不渲染（避免双标题）："重新开始"上移到本窗口
 * 标题栏，模型信息由底部 composer pill 承担，关闭走系统窗口控制。
 */
const launch = new URLSearchParams(window.location.search)
const ssid = launch.get("ssid")
const hostName = launch.get("title") ?? undefined

const { t } = useI18n()
const isMac = detectMac()

const aiRef = ref<InstanceType<typeof AiAssistant> | null>(null)

const windowTitle = computed(() =>
  hostName ? `${t("ai.title")} - ${hostName}` : t("ai.title"),
)

function onRequestNewChat() {
  aiRef.value?.openRestartConfirm()
}

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
  <div class="aiwin-root">
    <header class="aiwin-header" data-tauri-drag-region @dblclick="onHeaderDblClick">
      <WindowControls v-if="isMac" />
      <div class="brand" data-tauri-drag-region>
        <div class="brand-logo">
          <img src="/icon.png" alt="AShell" />
        </div>
        <span class="brand-name">{{ windowTitle }}</span>
      </div>
      <nav class="drag-spacer" data-tauri-drag-region />
      <NButton
        v-if="ssid"
        quaternary
        circle
        size="small"
        data-tauri-drag-region="false"
        :title="t('ai.restart')"
        @click="onRequestNewChat"
      >
        <template #icon>
          <NIcon><RefreshOutline /></NIcon>
        </template>
      </NButton>
      <WindowControls v-if="!isMac" />
    </header>
    <AiAssistant
      v-if="ssid"
      ref="aiRef"
      standalone
      :open="true"
      :sid="ssid"
      :host-name="hostName ?? null"
    />
  </div>
</template>

<style scoped>
.aiwin-root {
  height: 100vh;
  width: 100vw;
  background: var(--ashell-bg);
  --ashell-activity-w: 0px;
}

.aiwin-header {
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
