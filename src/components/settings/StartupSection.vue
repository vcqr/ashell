<script setup lang="ts">
import { computed, h, onMounted, ref, watch } from "vue";
import {
  NButton,
  NIcon,
  NInput,
  NSelect,
  NSwitch,
  type SelectOption,
  type SelectRenderTag,
} from "naive-ui";
import { RiArrowRightSLine, RiDeleteBinLine, RiTerminalBoxLine } from "@remixicon/vue";
import { useI18n } from "vue-i18n";
import { useStartupStore, shellOptionsForPlatform } from "@/stores/startup";
import { useTerminalProfilesStore } from "@/stores/terminalProfiles";
import { useIconStore } from "@/stores/icons";
import { previewShellCommand } from "@/api/local";

const { t } = useI18n();
const startupStore = useStartupStore();
const profilesStore = useTerminalProfilesStore();
const iconStore = useIconStore();

/* ---------- 自定义启动项：列表态 ⇆ 编辑态（主-详切换，避免一屏摊开所有表单） ---------- */
const editingProfileId = ref<string | null>(null);
const draft = ref({ name: "", command: "", cwd: "", icon: "" });

function openProfileEditor(id: string) {
  const p = profilesStore.list.find((x) => x.id === id);
  if (!p) return;
  editingProfileId.value = id;
  draft.value = {
    name: p.name,
    command: p.command,
    cwd: p.cwd ?? "",
    icon: p.icon ?? "",
  };
}

/** 添加：先建空项拿 id，直接进入编辑；命令留空的项不会出现在 + 菜单 */
function addProfile() {
  const id = profilesStore.add();
  openProfileEditor(id);
}

function saveProfile() {
  if (!editingProfileId.value) return;
  profilesStore.update(editingProfileId.value, { ...draft.value });
  editingProfileId.value = null;
}

function removeEditingProfile() {
  if (!editingProfileId.value) return;
  profilesStore.remove(editingProfileId.value);
  editingProfileId.value = null;
}

/* ---------- 自定义启动项的图标选择（复用 ~/.ashell/icons/ 图标库，与主机表单同源） ---------- */
const profileIconOptions = computed<SelectOption[]>(() => [
  { label: t("common.none"), value: "" },
  ...iconStore.items.map((it) => ({ label: it.name, value: it.name })),
]);

function renderIconThumb(name: string, size = 18) {
  const url = iconStore.urlOf(name);
  if (!url) return null;
  return h("img", {
    src: url,
    width: size,
    height: size,
    style: { borderRadius: "3px", objectFit: "contain", flexShrink: 0 },
  });
}

const renderProfileIconOption = (option: SelectOption) => {
  const name = option.value as string;
  return h(
    "div",
    {
      style: {
        display: "flex",
        alignItems: "center",
        gap: "8px",
        minHeight: "22px",
      },
    },
    [
      renderIconThumb(name),
      h("span", { style: { fontSize: "13px" } }, option.label as string),
    ],
  );
};

const renderProfileIconTag: SelectRenderTag = ({ option }) => {
  const name = option.value as string;
  return h(
    "div",
    { style: { display: "flex", alignItems: "center", gap: "8px" } },
    [renderIconThumb(name, 16), h("span", null, option.label as string)],
  );
};

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
onMounted(() => {
  void iconStore.ensureLoaded();
  void refreshShellPreview();
});
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
      <div class="setting-row setting-row-stacked">
        <div class="setting-row-main">
          <div class="setting-row-label">
            {{ t("settings.startup.profiles") }}
          </div>
          <div class="setting-row-desc">
            {{ t("settings.startup.profilesHint") }}
          </div>
        </div>
        <!-- 列表态：一行一项显示名称，点击进入编辑 -->
        <div v-if="editingProfileId === null" class="setting-row-control profile-list">
          <button
            v-for="p in profilesStore.list"
            :key="p.id"
            type="button"
            class="profile-row"
            @click="openProfileEditor(p.id)"
          >
            <img
              v-if="p.icon && iconStore.urlOf(p.icon)"
              :src="iconStore.urlOf(p.icon) ?? ''"
              class="profile-row-icon"
              alt=""
            />
            <NIcon v-else :size="16" class="profile-row-icon-default">
              <RiTerminalBoxLine />
            </NIcon>
            <span
              class="profile-row-name"
              :class="{ 'is-unnamed': !p.name.trim() }"
            >
              {{ p.name.trim() || t("settings.startup.profilesUnnamed") }}
            </span>
            <NIcon :size="14" class="profile-row-chevron">
              <RiArrowRightSLine />
            </NIcon>
          </button>
          <NButton dashed block size="small" @click="addProfile">
            {{ t("settings.startup.profilesAdd") }}
          </NButton>
        </div>
        <!-- 编辑态：整块切换为表单 -->
        <div v-else class="setting-row-control profile-form">
          <div class="profile-form-grid">
            <NInput
              :value="draft.name"
              :placeholder="t('settings.startup.profilesNamePlaceholder')"
              @update:value="(v: string) => (draft.name = v)"
            />
            <NSelect
              :value="draft.icon"
              :options="profileIconOptions"
              :render-label="renderProfileIconOption"
              :render-tag="renderProfileIconTag"
              filterable
              clearable
              :placeholder="t('settings.startup.profilesIconPlaceholder')"
              @update:value="(v: string | null) => (draft.icon = v ?? '')"
            />
            <NInput
              class="span-2"
              :value="draft.command"
              :placeholder="t('settings.startup.profilesCommandPlaceholder')"
              @update:value="(v: string) => (draft.command = v)"
            />
            <NInput
              class="span-2"
              :value="draft.cwd"
              :placeholder="t('settings.startup.profilesCwdPlaceholder')"
              @update:value="(v: string) => (draft.cwd = v)"
            />
          </div>
          <div class="profile-form-actions">
            <NButton
              quaternary
              type="error"
              size="small"
              @click="removeEditingProfile"
            >
              <template #icon>
                <NIcon :size="14"><RiDeleteBinLine /></NIcon>
              </template>
              {{ t("common.delete") }}
            </NButton>
            <NButton size="small" @click="editingProfileId = null">
              {{ t("common.cancel") }}
            </NButton>
            <NButton type="primary" size="small" @click="saveProfile">
              {{ t("common.save") }}
            </NButton>
          </div>
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

/* 自定义启动项——列表态：一行一项（图标 + 名称 + 箭头），点击进编辑 */
.profile-list {
  flex-direction: column;
  align-items: stretch;
  gap: 4px;
}

.profile-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--ashell-text);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.profile-row:hover {
  background: color-mix(in srgb, var(--ashell-text) 6%, transparent);
}

.profile-row-icon {
  width: 16px;
  height: 16px;
  object-fit: contain;
  border-radius: 3px;
  flex-shrink: 0;
}

.profile-row-icon-default {
  color: var(--ashell-text-muted);
  flex-shrink: 0;
}

.profile-row-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-row-name.is-unnamed {
  color: var(--ashell-text-muted);
}

.profile-row-chevron {
  color: var(--ashell-text-muted);
  flex-shrink: 0;
}

/* 自定义启动项——编辑态：名称/图标同行，命令与工作目录整行，底部操作条 */
.profile-form {
  flex-direction: column;
  align-items: stretch;
  gap: 10px;
}

.profile-form-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 240px;
  gap: 8px;
}

.profile-form-grid .span-2 {
  grid-column: 1 / -1;
}

.profile-form-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.profile-form-actions > :first-child {
  margin-right: auto;
}
</style>
