<script setup lang="ts">
import { computed } from "vue";
import { NInput, NSelect, NSwitch } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useStartupStore, shellOptionsForPlatform } from "@/stores/startup";

const { t } = useI18n();
const startupStore = useStartupStore();

const shellOptions = computed(() =>
  shellOptionsForPlatform().map((o) => ({ label: o.label, value: o.value })),
);
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.tabs.startup") }}</div>
    <div class="settings-block">
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.rememberTabs") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.rememberTabsHint") }}
          </div>
        </div>
        <div class="setting-row-control">
          <NSwitch
            :value="startupStore.restoreTabs"
            @update:value="(v: boolean) => startupStore.setRestoreTabs(v)"
          />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.autoConnectRememberedTabs") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.autoConnectRememberedTabsHint") }}
          </div>
        </div>
        <div class="setting-row-control">
          <NSwitch
            :value="startupStore.autoConnectRememberedTabs"
            :disabled="!startupStore.restoreTabs"
            @update:value="
              (v: boolean) => startupStore.setAutoConnectRememberedTabs(v)
            "
          />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.autoOpenLocal") }}
          </div>
        </div>
        <div class="setting-row-control">
          <NSwitch
            :value="startupStore.openLocalOnStart"
            @update:value="(v: boolean) => startupStore.setOpenLocalOnStart(v)"
          />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.defaultShell") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.defaultShellHint") }}
          </div>
        </div>
        <div class="setting-row-control w-shell">
          <NSelect
            :value="startupStore.defaultShell"
            :options="shellOptions"
            filterable
            tag
            @update:value="(v: string) => startupStore.setDefaultShell(v)"
          />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.localInitCommand") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.localInitCommandHint") }}
          </div>
        </div>
        <div class="setting-row-control w-cmd">
          <NInput
            :value="startupStore.localInitCommand"
            :placeholder="t('settings.startup.localInitCommandPlaceholder')"
            @update:value="(v: string) => startupStore.setLocalInitCommand(v)"
          />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.cwdFollow") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.cwdFollowHint") }}
          </div>
        </div>
        <div class="setting-row-control">
          <NSwitch
            :value="startupStore.cwdFollowEnabled"
            @update:value="(v: boolean) => startupStore.setCwdFollowEnabled(v)"
          />
        </div>
      </div>
      <div class="setting-row">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.sftpCwdFollow") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.sftpCwdFollowHint") }}
          </div>
        </div>
        <div class="setting-row-control">
          <NSwitch
            :value="startupStore.sftpCwdFollowEnabled"
            @update:value="
              (v: boolean) => startupStore.setSftpCwdFollowEnabled(v)
            "
          />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* 分组标题、页面骨架样式已抽到 src/assets/main.css 统一维护 */

/* 默认 Shell 下拉（可输入自定义 shell），与 Terminal 字体下拉同宽 */
.w-shell {
  width: 320px;
}

/* 本地终端初始命令输入框 */
.w-cmd {
  width: 320px;
}
</style>
