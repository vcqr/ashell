<script setup lang="ts">
import { computed } from "vue";
import { NForm, NFormItem, NSelect, NSwitch } from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  useStartupStore,
  shellOptionsForPlatform,
} from "@/stores/startup";

const { t } = useI18n();
const startupStore = useStartupStore();

const shellOptions = computed(() =>
  shellOptionsForPlatform().map((o) => ({ label: o.label, value: o.value })),
);
</script>

<template>
  <NForm label-placement="top" require-mark-placement="left">
    <NFormItem
      :label="t('settings.startup.rememberTabs')"
      :feedback="t('settings.startup.rememberTabsHint')"
    >
      <NSwitch
        :value="startupStore.restoreTabs"
        @update:value="(v: boolean) => startupStore.setRestoreTabs(v)"
      />
    </NFormItem>
    <NFormItem
      :label="t('settings.startup.autoConnectRememberedTabs')"
      :feedback="t('settings.startup.autoConnectRememberedTabsHint')"
    >
      <NSwitch
        :value="startupStore.autoConnectRememberedTabs"
        :disabled="!startupStore.restoreTabs"
        @update:value="(v: boolean) => startupStore.setAutoConnectRememberedTabs(v)"
      />
    </NFormItem>
    <NFormItem :label="t('settings.startup.autoOpenLocal')">
      <NSwitch
        :value="startupStore.openLocalOnStart"
        @update:value="(v: boolean) => startupStore.setOpenLocalOnStart(v)"
      />
    </NFormItem>
    <NFormItem
      :label="t('settings.startup.defaultShell')"
      :feedback="t('settings.startup.defaultShellHint')"
    >
      <NSelect
        :value="startupStore.defaultShell"
        :options="shellOptions"
        filterable
        tag
        @update:value="(v: string) => startupStore.setDefaultShell(v)"
      />
    </NFormItem>
  </NForm>
</template>
