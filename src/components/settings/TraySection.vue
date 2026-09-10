<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import {
  NButton,
  NForm,
  NFormItem,
  NRadio,
  NRadioGroup,
  NSwitch,
  NTag,
  NText,
  useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { useTrayStore, type TrayCloseAction } from "@/stores/tray";
import { useHotkeyStore } from "@/stores/hotkey";

const { t } = useI18n();
const trayStore = useTrayStore();
const hotkeyStore = useHotkeyStore();
const message = useMessage();

const recording = ref(false);

// e.code 与键盘布局无关，保证录到的键位与 global-hotkey 解析结果一致
const CODE_MAP: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Insert: "Insert",
  Minus: "Minus",
  Equal: "Equal",
  Comma: "Comma",
  Period: "Period",
  Slash: "Slash",
  Backslash: "Backslash",
  Semicolon: "Semicolon",
  Quote: "Quote",
  Backquote: "Backquote",
  BracketLeft: "BracketLeft",
  BracketRight: "BracketRight",
};

/** 把按键事件转成 global-hotkey 格式（如 Ctrl+Shift+K）；不合格的组合返回 null 继续等待 */
function toAccelerator(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
  // F1-F24 是功能键不参与打字，可单独作为全局热键；
  // 其余按键必须带 Ctrl/Alt/Meta，否则会劫持正常输入
  const isFunctionKey = /^F(?:[1-9]|1\d|2[0-4])$/.test(e.code);
  if (!e.ctrlKey && !e.altKey && !e.metaKey && !isFunctionKey) return null;
  let key: string | null = null;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (/^F(?:[1-9]|1\d|2[0-4])$/.test(e.code)) key = e.code;
  else key = CODE_MAP[e.code] ?? null;
  if (!key) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Meta");
  parts.push(key);
  return parts.join("+");
}

async function saveHotkey(enabled: boolean, accelerator: string) {
  const err = await hotkeyStore.save(enabled, accelerator);
  if (err) message.error(t("settings.tray.hotkeyFailed", { error: err }));
}

async function startRecording() {
  if (recording.value) {
    // 再点一次 = 取消录制
    recording.value = false;
    await hotkeyStore.resume();
    return;
  }
  recording.value = true;
  // 临时注销已注册的热键：否则它会把按下的同名组合在 OS 层拦截，
  // 录制器收不到，还会触发一次唤起/隐藏
  if (hotkeyStore.enabled) await hotkeyStore.suspend();
}

async function clearHotkey() {
  recording.value = false;
  await saveHotkey(false, "");
}

function onRecordKeydown(e: KeyboardEvent) {
  if (!recording.value) return;
  e.preventDefault();
  e.stopPropagation();

  if (e.key === "Escape") {
    recording.value = false;
    void hotkeyStore.resume();
    return;
  }

  if (e.key === "Backspace" || e.key === "Delete") {
    recording.value = false;
    void saveHotkey(false, "");
    return;
  }

  const acc = toAccelerator(e);
  if (!acc) return;
  recording.value = false;
  // 保存成功后端已自行注册新键并持久化，无需 resume
  void saveHotkey(true, acc);
}

async function onHotkeyEnable(v: boolean) {
  // 开启但还没录按键时直接进入录制，录好即自动生效
  if (v && !hotkeyStore.accelerator) {
    recording.value = true;
    return;
  }
  await saveHotkey(v, hotkeyStore.accelerator);
}

async function onAutostartChange(v: boolean) {
  const result = await trayStore.setAutostart(v);
  if (result === null) {
    message.error(t("settings.tray.autostartFailed"));
  }
}

onMounted(() => {
  void trayStore.load();
  void hotkeyStore.load();
  window.addEventListener("keydown", onRecordKeydown, true);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onRecordKeydown, true);
  if (recording.value) {
    // 卸载时仍在录制：恢复被临时注销的热键
    void hotkeyStore.resume();
  }
  recording.value = false;
});
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.tray.title") }}</div>
    <NForm label-placement="top" size="small" :show-feedback="false">
      <NFormItem :label="t('settings.tray.enable')">
        <NSwitch
          :value="trayStore.enabled"
          @update:value="(v: boolean) => trayStore.setEnabled(v)"
        />
      </NFormItem>
      <p class="settings-hint">{{ t("settings.tray.enableHint") }}</p>

      <NFormItem :label="t('settings.tray.closeAction')">
        <NRadioGroup
          :value="trayStore.closeAction"
          :disabled="!trayStore.enabled"
          @update:value="(v: TrayCloseAction) => trayStore.setCloseAction(v)"
        >
          <div class="tray-radio-col">
            <NRadio value="quit">{{ t("settings.tray.closeActionQuit") }}</NRadio>
            <NRadio value="hide">{{ t("settings.tray.closeActionHide") }}</NRadio>
          </div>
        </NRadioGroup>
      </NFormItem>
      <p class="settings-hint">{{ t("settings.tray.closeActionHint") }}</p>

      <NFormItem :label="t('settings.tray.autostart')">
        <NSwitch :value="trayStore.autostart" @update:value="onAutostartChange" />
      </NFormItem>
      <p class="settings-hint">{{ t("settings.tray.autostartHint") }}</p>

      <NFormItem :label="t('settings.tray.hotkey')">
        <div class="hotkey-row">
          <NSwitch :value="hotkeyStore.enabled" @update:value="onHotkeyEnable" />
          <div
            class="hotkey-binding"
            :class="{ recording }"
            @click="startRecording"
          >
            <span v-if="recording" class="hotkey-recording-hint">
              {{ t("settings.tray.hotkeyRecording") }}
            </span>
            <template v-else-if="hotkeyStore.accelerator">
              <NTag
                v-for="part in hotkeyStore.accelerator.split('+')"
                :key="part"
                size="small"
                :bordered="true"
                type="info"
              >
                {{ part }}
              </NTag>
            </template>
            <span v-else class="hotkey-unset">
              {{ t("settings.tray.hotkeyNotSet") }}
            </span>
          </div>
          <NButton
            size="tiny"
            :type="recording ? 'warning' : 'default'"
            secondary
            @click="startRecording"
          >
            {{
              recording
                ? t("settings.tray.hotkeyCancel")
                : t("settings.tray.hotkeyRecord")
            }}
          </NButton>
          <NButton
            v-if="!recording && hotkeyStore.accelerator"
            size="tiny"
            quaternary
            type="error"
            @click="clearHotkey"
          >
            {{ t("settings.tray.hotkeyClear") }}
          </NButton>
        </div>
      </NFormItem>
      <p class="settings-hint">{{ t("settings.tray.hotkeyHint") }}</p>

      <NText depth="3" class="tray-menu-hint">
        {{ t("settings.tray.menuHint") }}
      </NText>
    </NForm>
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

.settings-hint {
  margin: 0 0 12px;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.6;
}

.tray-radio-col {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.hotkey-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.hotkey-binding {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 140px;
  min-height: 28px;
  padding: 2px 10px;
  border: 1px solid var(--ashell-border, rgba(255, 255, 255, 0.12));
  border-radius: 6px;
  cursor: pointer;
  transition: border-color 0.15s ease;
}

.hotkey-binding.recording {
  border-color: var(--ashell-primary, #6366f1);
}

.hotkey-unset,
.hotkey-recording-hint {
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
}

.tray-menu-hint {
  display: block;
  font-size: 12px;
  line-height: 1.6;
}
</style>
