<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { NButton, NInput, NSelect, NSwitch } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useStartupStore, shellOptionsForPlatform } from "@/stores/startup";
import { previewShellCommand } from "@/api/local";

const { t } = useI18n();
const startupStore = useStartupStore();

const shellOptions = computed(() =>
  shellOptionsForPlatform().map((o) => ({ label: o.label, value: o.value })),
);

// 当前 shell 类型对应的默认启动命令（后端真实解析，含 Git Bash 路径探测等），
// 用作输入框 placeholder 与「填充默认」的数据源；拉取失败静默留空
const shellPreview = ref("");
async function refreshShellPreview() {
  const prev = shellPreview.value;
  try {
    const { command } = await previewShellCommand(
      startupStore.defaultShell || undefined,
    );
    shellPreview.value = command;
    // 输入框内容还停留在上一类型的默认命令（如「填充默认」带入、未曾真正
    // 改过参数）时，跟随新类型刷新，避免切换类型后框里一直显示旧命令；
    // 首次拉取（prev 为空）不回填，保持「留空 = 跟随默认」的语义
    if (prev && command && startupStore.shellCommand === prev) {
      startupStore.setShellCommand(command);
    }
  } catch {
    shellPreview.value = "";
  }
}
onMounted(refreshShellPreview);
watch(() => startupStore.defaultShell, refreshShellPreview);

/** 「填充默认」：把当前默认命令写进输入框，方便在此基础上改参数 */
function fillDefaultCommand() {
  startupStore.setShellCommand(shellPreview.value);
}
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
      <div class="setting-row setting-row-stacked">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.shellCommand") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.shellCommandHint") }}
          </div>
        </div>
        <div class="setting-row-control">
          <NInput
            :value="startupStore.shellCommand"
            clearable
            :placeholder="shellPreview"
            @update:value="(v: string) => startupStore.setShellCommand(v)"
          >
            <template #suffix>
              <NButton
                quaternary
                size="tiny"
                :disabled="!shellPreview"
                :title="t('settings.startup.shellCommandFillDefault')"
                @click="fillDefaultCommand"
              >
                {{ t("settings.startup.shellCommandFillDefault") }}
              </NButton>
            </template>
          </NInput>
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

/* 启动命令行：上下堆叠——命令行普遍较长，左右布局的 320px 放不下，
   改为标签在上、输入框独占整行 */
.setting-row-stacked {
  flex-direction: column;
  align-items: stretch;
  gap: 6px;
}

.setting-row-stacked .setting-row-control {
  width: 100%;
}
</style>
