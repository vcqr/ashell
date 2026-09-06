<script setup lang="ts">
import { onMounted } from "vue";
import {
  NForm,
  NFormItem,
  NRadio,
  NRadioGroup,
  NSwitch,
  NText,
  useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { useTrayStore, type TrayCloseAction } from "@/stores/tray";

const { t } = useI18n();
const trayStore = useTrayStore();
const message = useMessage();

onMounted(() => {
  void trayStore.load();
});

async function onAutostartChange(v: boolean) {
  const result = await trayStore.setAutostart(v);
  if (result === null) {
    message.error(t("settings.tray.autostartFailed"));
  }
}
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

.tray-menu-hint {
  display: block;
  font-size: 12px;
  line-height: 1.6;
}
</style>
