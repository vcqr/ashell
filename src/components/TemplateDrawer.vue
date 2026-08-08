<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import {
  NButton,
  NCard,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NModal,
  NSelect,
  NSpace,
  NScrollbar,
  useMessage,
} from "naive-ui";
import {
  AddOutline,
  CloseOutline,
  CreateOutline,
  TrashOutline,
  TerminalOutline,
  SearchOutline,
} from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import { useTemplateStore } from "@/stores/templates";
import { getRecentCommands } from "@/composables/useCommandSuggest";
import type { CommandTemplate } from "@/types";

const props = defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
  "run": [command: string];
}>();

const { t } = useI18n();
const message = useMessage();
const store = useTemplateStore();

const search = ref("");
const showForm = ref(false);
const editingId = ref<number | null>(null);
const formTitle = ref("");
const formCommand = ref("");
const formDescription = ref("");

const historyOptions = computed(() =>
  getRecentCommands(20).map((cmd) => ({
    label: cmd,
    value: cmd,
  })),
);

function onPickHistory(value: string) {
  formCommand.value = value;
}

const filtered = computed(() => {
  const q = search.value.trim().toLowerCase();
  if (!q) return store.templates;
  return store.templates.filter(
    (t) =>
      t.title.toLowerCase().includes(q) ||
      t.command.toLowerCase().includes(q) ||
      (t.description ?? "").toLowerCase().includes(q),
  );
});

onMounted(() => {
  store.load();
});

watch(
  () => props.open,
  (open) => {
    if (open) store.load();
  },
);

function onClose() {
  emit("update:open", false);
}

function runCommand(cmd: string) {
  emit("run", cmd);
}

function openCreate() {
  editingId.value = null;
  formTitle.value = "";
  formCommand.value = "";
  formDescription.value = "";
  showForm.value = true;
}

function openEdit(tpl: CommandTemplate) {
  editingId.value = tpl.id;
  formTitle.value = tpl.title;
  formCommand.value = tpl.command;
  formDescription.value = tpl.description ?? "";
  showForm.value = true;
}

async function submitForm() {
  const title = formTitle.value.trim();
  const command = formCommand.value.trim();
  if (!title) {
    message.warning(t("templates.message.titleRequired"));
    return;
  }
  if (!command) {
    message.warning(t("templates.message.commandRequired"));
    return;
  }

  if (editingId.value !== null) {
    const ok = await store.edit(editingId.value, {
      title,
      command,
      description: formDescription.value.trim() || null,
    });
    if (ok) message.success(t("templates.message.updated"));
    else message.error(t("templates.message.updateFailed"));
  } else {
    const ok = await store.add({
      title,
      command,
      description: formDescription.value.trim() || null,
    });
    if (ok) message.success(t("templates.message.created"));
    else message.error(t("templates.message.createFailed"));
  }
  showForm.value = false;
}

async function onDelete(id: number) {
  await store.remove(id);
  message.success(t("templates.message.deleted"));
}
</script>

<template>
  <Teleport to="body">
    <aside
      class="tpl-panel"
      :class="{ open: props.open }"
      :aria-hidden="!props.open"
    >
      <header class="panel-header">
        <NSpace align="center" :size="8">
          <NIcon :size="16"><TerminalOutline /></NIcon>
          <span class="drawer-title">{{ t("templates.title") }}</span>
        </NSpace>
        <NSpace :size="6" align="center" :wrap="false">
          <NButton size="small" type="primary" @click="openCreate">
            <template #icon>
              <NIcon><AddOutline /></NIcon>
            </template>
            {{ t("templates.newButton") }}
          </NButton>
          <NButton
            size="small"
            quaternary
            circle
            :title="t('templates.close')"
            @click="onClose"
          >
            <template #icon>
              <NIcon><CloseOutline /></NIcon>
            </template>
          </NButton>
        </NSpace>
      </header>

      <div class="search-bar">
        <NInput
          v-model:value="search"
          :placeholder="t('templates.searchPlaceholder')"
          clearable
          size="small"
        >
          <template #prefix>
            <NIcon :size="14"><SearchOutline /></NIcon>
          </template>
        </NInput>
      </div>

      <div class="panel-body">
        <NScrollbar>
          <div v-if="filtered.length === 0" class="empty-state">
            {{ store.templates.length === 0 ? t("templates.empty") : t("templates.noMatch") }}
          </div>
          <div v-else class="tpl-list">
            <div
              v-for="tpl in filtered"
              :key="tpl.id"
              class="tpl-item"
            >
              <div class="tpl-info" @click="runCommand(tpl.command)">
                <div class="tpl-title">{{ tpl.title }}</div>
                <div class="tpl-command"><code>{{ tpl.command }}</code></div>
                <div v-if="tpl.description" class="tpl-desc">{{ tpl.description }}</div>
              </div>
              <div class="tpl-actions">
                <button
                  class="tpl-action-btn"
                  :title="t('templates.edit')"
                  @click="openEdit(tpl)"
                >
                  <NIcon :size="14"><CreateOutline /></NIcon>
                </button>
                <button
                  class="tpl-action-btn danger"
                  :title="t('templates.delete')"
                  @click="onDelete(tpl.id)"
                >
                  <NIcon :size="14"><TrashOutline /></NIcon>
                </button>
              </div>
            </div>
          </div>
        </NScrollbar>
      </div>

      <!-- Create / Edit modal -->
      <NModal v-model:show="showForm">
        <NCard
          style="width: min(460px, 90vw)"
          :title="editingId !== null ? t('templates.editTitle') : t('templates.newTitle')"
          :bordered="false"
          role="dialog"
          aria-modal="true"
        >
          <NForm label-placement="top" size="small">
            <NFormItem :label="t('templates.form.title')">
              <NInput
                v-model:value="formTitle"
                :placeholder="t('templates.form.titlePlaceholder')"
              />
            </NFormItem>
            <NFormItem :label="t('templates.form.command')">
              <NInput
                v-model:value="formCommand"
                type="textarea"
                :autosize="{ minRows: 2, maxRows: 6 }"
                :placeholder="t('templates.form.commandPlaceholder')"
              />
            </NFormItem>
            <NFormItem v-if="historyOptions.length > 0" :label="t('templates.form.pickFromHistory')">
              <NSelect
                :options="historyOptions"
                :placeholder="t('templates.form.historyPlaceholder')"
                filterable
                clearable
                :value="null"
                @update:value="onPickHistory"
              />
            </NFormItem>
            <NFormItem :label="t('templates.form.description')">
              <NInput
                v-model:value="formDescription"
                :placeholder="t('templates.form.descriptionPlaceholder')"
              />
            </NFormItem>
          </NForm>
          <template #footer>
            <NSpace justify="end">
              <NButton @click="showForm = false">{{ t("templates.form.cancel") }}</NButton>
              <NButton type="primary" @click="submitForm">{{ t("templates.form.submit") }}</NButton>
            </NSpace>
          </template>
        </NCard>
      </NModal>
    </aside>
  </Teleport>
</template>

<style scoped>
.tpl-panel {
  position: fixed;
  top: var(--ashell-header-h);
  right: var(--ashell-activity-w, 0px);
  bottom: 0;
  width: 360px;
  background: var(--ashell-panel-bg);
  border-left: 1px solid var(--ashell-border);
  box-shadow: -8px 0 24px var(--ashell-shadow);
  display: flex;
  flex-direction: column;
  z-index: 1000;
  transform: translateX(100%);
  transition: transform 0.25s ease;
  user-select: text;
}

.tpl-panel.open {
  transform: translateX(0);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

.drawer-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--ashell-text-strong);
}

.search-bar {
  padding: 8px 14px;
  border-bottom: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

.panel-body {
  flex: 1;
  min-height: 0;
}

.empty-state {
  text-align: center;
  font-size: 13px;
  color: var(--ashell-text-subtle);
  padding: 32px 0;
}

.tpl-list {
  display: flex;
  flex-direction: column;
  padding: 8px;
  gap: 4px;
}

.tpl-item {
  display: flex;
  align-items: stretch;
  gap: 4px;
  padding: 8px 10px;
  border-radius: 8px;
  transition: background 0.15s ease;
}

.tpl-item:hover {
  background: color-mix(in srgb, var(--ashell-primary) 10%, transparent);
}

.tpl-info {
  flex: 1;
  min-width: 0;
  cursor: pointer;
}

.tpl-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  margin-bottom: 2px;
}

.tpl-command {
  font-size: 12px;
  color: var(--ashell-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tpl-command code {
  font-family: var(--ashell-mono, "Fira Code", "JetBrains Mono", Menlo, Consolas, monospace);
  font-size: 12px;
}

.tpl-desc {
  font-size: 11px;
  color: var(--ashell-text-subtle);
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tpl-actions {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity 0.15s ease;
}

.tpl-item:hover .tpl-actions {
  opacity: 1;
}

.tpl-action-btn {
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 5px;
  background: transparent;
  cursor: pointer;
  color: var(--ashell-text-subtle);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: color 0.15s ease, background 0.15s ease;
}

.tpl-action-btn:hover {
  background: color-mix(in srgb, var(--ashell-primary) 18%, transparent);
  color: var(--ashell-text-strong);
}

.tpl-action-btn.danger:hover {
  background: color-mix(in srgb, #ef4444 20%, transparent);
  color: #ef4444;
}
</style>
