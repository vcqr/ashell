<script setup lang="ts">
import { ref, reactive, onMounted } from "vue";
import {
  NForm,
  NFormItem,
  NInput,
  NButton,
  NIcon,
  NSpin,
  NEmpty,
  NPopconfirm,
  NText,
  NModal,
  NCard,
  useMessage,
} from "naive-ui";
import {
  CloudUploadOutline,
  DownloadOutline,
  FolderOpenOutline,
  SaveOutline,
  CloudOutline,
  RefreshOutline,
} from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import { invoke } from "@tauri-apps/api/core";
import {
  getBackupConfig,
  saveBackupConfig,
  testBackupConnection,
  createBackup,
  exportBackup,
  importBackup,
  deleteBackup,
  listBackups,
  restoreBackup,
  type BackupConfig,
  type BackupItem,
} from "@/api/backup";

const { t } = useI18n();
const message = useMessage();

const config = ref<BackupConfig>({
  endpoint: "",
  bucket: "",
  region: "",
  access_key: "",
  secret_key: "",
  path_prefix: "ashell/",
});

const loading = ref(false);
const saving = ref(false);
const testing = ref(false);
const backing = ref(false);
const exporting = ref(false);
const importing = ref(false);
const restoring = ref(false);
const deleting = ref(false);
const backups = ref<BackupItem[]>([]);

// ── Password dialog ──

const pwdDialog = reactive({
  show: false,
  title: "",
  hint: "",
  input: "",
  resolve: null as ((pwd: string) => void) | null,
  reject: null as (() => void) | null,
});

function requestPassword(title: string, hint?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    pwdDialog.title = title;
    pwdDialog.hint = hint ?? "";
    pwdDialog.input = "";
    pwdDialog.show = true;
    pwdDialog.resolve = resolve;
    pwdDialog.reject = reject;
  });
}

function confirmPassword() {
  if (!pwdDialog.input.trim()) return;
  pwdDialog.show = false;
  pwdDialog.resolve?.(pwdDialog.input);
  pwdDialog.resolve = null;
  pwdDialog.reject = null;
}

function cancelPassword() {
  pwdDialog.show = false;
  pwdDialog.reject?.();
  pwdDialog.resolve = null;
  pwdDialog.reject = null;
}

// ── Lifecycle ──

onMounted(async () => {
  await loadConfig();
  if (config.value.bucket && config.value.access_key) {
    loadBackups();
  }
});

// ── Config ──

async function loadConfig() {
  loading.value = true;
  try {
    config.value = await getBackupConfig();
  } catch {
    // use defaults
  } finally {
    loading.value = false;
  }
}

async function handleSave() {
  saving.value = true;
  try {
    await saveBackupConfig({
      endpoint: config.value.endpoint.trim(),
      bucket: config.value.bucket.trim(),
      region: config.value.region.trim(),
      access_key: config.value.access_key.trim(),
      secret_key: config.value.secret_key.trim(),
      path_prefix: config.value.path_prefix.trim(),
    });
    message.success(t("settings.backup.saveSuccess"));
  } catch (e) {
    message.error(t("settings.backup.saveFailed", { error: String(e) }));
  } finally {
    saving.value = false;
  }
}

async function handleTest() {
  testing.value = true;
  try {
    await testBackupConnection({
      endpoint: config.value.endpoint.trim(),
      bucket: config.value.bucket.trim(),
      region: config.value.region.trim(),
      access_key: config.value.access_key.trim(),
      secret_key: config.value.secret_key.trim(),
      path_prefix: config.value.path_prefix.trim(),
    });
    message.success(t("settings.backup.testSuccess"));
  } catch (e) {
    message.error(t("settings.backup.testFailed", { error: String(e) }));
  } finally {
    testing.value = false;
  }
}

// ── Backup operations ──

function getCommandHistory(): string[] {
  try {
    const raw = localStorage.getItem("ashell-command-history");
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (Array.isArray(arr)) {
      return arr.filter((s): s is string => typeof s === "string");
    }
  } catch {
    // ignore
  }
  return [];
}

async function handleBackup() {
  try {
    const password = await requestPassword(
      t("settings.backup.pwdBackupTitle"),
      t("settings.backup.pwdBackupHint"),
    );
    backing.value = true;
    const commandHistory = getCommandHistory();
    await createBackup(commandHistory, password);
    message.success(t("settings.backup.backupSuccess"));
    await loadBackups();
  } catch (e) {
    if (e !== undefined) message.error(t("settings.backup.backupFailed", { error: String(e) }));
  } finally {
    backing.value = false;
  }
}

async function loadBackups() {
  try {
    backups.value = await listBackups();
  } catch (e) {
    message.error(t("settings.backup.loadFailed", { error: String(e) }));
  }
}

async function handleExport() {
  try {
    const password = await requestPassword(
      t("settings.backup.pwdExportTitle"),
      t("settings.backup.pwdExportHint"),
    );
    exporting.value = true;
    const commandHistory = getCommandHistory();
    const result = await exportBackup(commandHistory, password);
    const now = new Date();
    const ts = now.toISOString().replace(/[:.]/g, "").slice(0, -1);
    const savedPath = await invoke<string | null>("save_text_file", {
      defaultFilename: `ashell-backup-${ts}.json`,
      content: result.content,
    });
    if (savedPath) {
      message.success(t("settings.backup.exportSuccess"));
    }
  } catch (e) {
    if (e !== undefined) message.error(t("settings.backup.exportFailed", { error: String(e) }));
  } finally {
    exporting.value = false;
  }
}

async function handleImport() {
  try {
    const content = await invoke<string | null>("open_text_file");
    if (!content) return;
    const password = await requestPassword(
      t("settings.backup.pwdImportTitle"),
      t("settings.backup.pwdImportHint"),
    );
    importing.value = true;
    const result = await importBackup(content, password);
    try {
      localStorage.setItem(
        "ashell-command-history",
        JSON.stringify(result.command_history),
      );
    } catch {
      // ignore
    }
    message.success(t("settings.backup.restoreSuccess"));
    setTimeout(() => window.location.reload(), 1500);
  } catch (e) {
    if (e !== undefined) message.error(t("settings.backup.restoreFailed", { error: String(e) }));
  } finally {
    importing.value = false;
  }
}

async function handleRestore(key: string) {
  try {
    const password = await requestPassword(
      t("settings.backup.pwdRestoreTitle"),
      t("settings.backup.pwdRestoreHint"),
    );
    restoring.value = true;
    const result = await restoreBackup(key, password);
    try {
      localStorage.setItem(
        "ashell-command-history",
        JSON.stringify(result.command_history),
      );
    } catch {
      // ignore
    }
    message.success(t("settings.backup.restoreSuccess"));
    setTimeout(() => window.location.reload(), 1500);
  } catch (e) {
    if (e !== undefined) message.error(t("settings.backup.restoreFailed", { error: String(e) }));
  } finally {
    restoring.value = false;
  }
}

async function handleDelete(key: string) {
  deleting.value = true;
  try {
    await deleteBackup(key);
    message.success(t("settings.backup.deleteSuccess"));
    await loadBackups();
  } catch (e) {
    message.error(t("settings.backup.deleteFailed", { error: String(e) }));
  } finally {
    deleting.value = false;
  }
}

// ── Utils ──

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTime(timestamp: string): string {
  const date = new Date(timestamp.replace(/(\d{4})-(\d{2})-(\d{2})T(\d{2})(\d{2})(\d{2})Z/, "$1-$2-$3T$4:$5:$6Z"))
  if (isNaN(date.getTime())) return timestamp
  return date.toLocaleString()
}
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.backup.title") }}</div>

    <NSpin :show="loading" size="small">
      <NForm label-placement="top" size="small" :show-feedback="false">
        <div class="settings-subgroup">{{ t("settings.backup.configTitle") }}</div>
        <NFormItem :label="t('settings.backup.endpoint')" style="margin-top: 12px">
          <NInput v-model:value="config.endpoint" placeholder="https://s3.amazonaws.com" />
        </NFormItem>
        <NFormItem :label="t('settings.backup.bucket')" style="margin-top: 12px">
          <NInput v-model:value="config.bucket" placeholder="my-backup-bucket" />
        </NFormItem>
        <div class="form-row" style="margin-top: 12px">
          <NFormItem :label="t('settings.backup.region')" style="flex: 1">
            <NInput v-model:value="config.region" placeholder="us-east-1" />
          </NFormItem>
          <NFormItem :label="t('settings.backup.pathPrefix')" style="flex: 1">
            <NInput v-model:value="config.path_prefix" placeholder="ashell/" />
          </NFormItem>
        </div>
        <NFormItem :label="t('settings.backup.accessKey')" style="margin-top: 12px">
          <NInput v-model:value="config.access_key" placeholder="AKIAIOSFODNN7EXAMPLE" />
        </NFormItem>
        <NFormItem :label="t('settings.backup.secretKey')" style="margin-top: 12px">
          <NInput
            v-model:value="config.secret_key"
            type="password"
            show-password-on="click"
            placeholder="••••••••••••••••"
          />
        </NFormItem>
      </NForm>
      <p class="settings-hint">{{ t("settings.backup.pathPrefixHint") }}</p>
      <div class="form-row" style="margin-top: 12px">
        <NButton size="small" :loading="saving" @click="handleSave">
          <template #icon><NIcon><SaveOutline /></NIcon></template>
          {{ t("settings.backup.saveConfig") }}
        </NButton>
        <NButton size="small" :loading="testing" @click="handleTest">
          <template #icon><NIcon><CloudOutline /></NIcon></template>
          {{ t("settings.backup.testConnection") }}
        </NButton>
      </div>

      <div class="settings-subgroup" style="margin-top: 16px">{{ t("settings.backup.operationTitle") }}</div>
      <div class="form-row" style="margin-top: 12px">
        <NButton type="primary" size="small" :loading="backing" @click="handleBackup">
          <template #icon><NIcon><CloudUploadOutline /></NIcon></template>
          {{ t("settings.backup.createBackup") }}
        </NButton>
        <NButton size="small" :loading="exporting" @click="handleExport">
          <template #icon><NIcon><DownloadOutline /></NIcon></template>
          {{ t("settings.backup.exportLocal") }}
        </NButton>
        <NButton size="small" :loading="importing" @click="handleImport">
          <template #icon><NIcon><FolderOpenOutline /></NIcon></template>
          {{ t("settings.backup.importLocal") }}
        </NButton>
      </div>

      <div class="settings-subgroup" style="margin-top: 16px">{{ t("settings.backup.historyTitle") }}</div>
      <div class="history-header" style="margin-top: 12px">
        <NButton quaternary size="tiny" :loading="loading" @click="loadBackups">
          <template #icon><NIcon><RefreshOutline /></NIcon></template>
          {{ t("settings.backup.refresh") }}
        </NButton>
      </div>
      <NEmpty
        v-if="backups.length === 0 && !loading"
        :description="t('settings.backup.noBackups')"
        size="small"
        style="margin-top: 8px"
      />
      <div v-else class="backup-list" style="margin-top: 8px">
        <div v-for="item in backups" :key="item.key" class="backup-item">
          <div class="backup-info">
            <div class="backup-name">{{ formatTime(item.timestamp) }}</div>
            <NText depth="3" class="backup-size">{{ formatSize(item.size) }}</NText>
          </div>
          <div class="backup-actions">
            <NPopconfirm @positive-click="handleRestore(item.key)">
              <template #trigger>
                <NButton size="tiny" type="warning">
                  {{ t("settings.backup.restore") }}
                </NButton>
              </template>
              {{ t("settings.backup.restoreConfirm") }}
            </NPopconfirm>
            <NPopconfirm @positive-click="handleDelete(item.key)">
              <template #trigger>
                <NButton size="tiny" type="error">
                  {{ t("settings.backup.delete") }}
                </NButton>
              </template>
              {{ t("settings.backup.deleteConfirm") }}
            </NPopconfirm>
          </div>
        </div>
      </div>
    </NSpin>

    <!-- Password Dialog -->
    <NModal :show="pwdDialog.show" @update:show="(v: boolean) => !v && cancelPassword()">
      <NCard
        :title="pwdDialog.title"
        size="small"
        style="width: 360px"
        :bordered="false"
        role="dialog"
        aria-modal="true"
      >
        <p v-if="pwdDialog.hint" class="pwd-hint">{{ pwdDialog.hint }}</p>
        <NInput
          v-model:value="pwdDialog.input"
          type="password"
          show-password-on="click"
          :placeholder="t('settings.backup.pwdPlaceholder')"
          @keydown.enter="confirmPassword"
        />
        <template #footer>
          <div class="pwd-actions">
            <NButton size="small" @click="cancelPassword">{{ t("settings.backup.pwdCancel") }}</NButton>
            <NButton size="small" type="primary" @click="confirmPassword">{{ t("settings.backup.pwdConfirm") }}</NButton>
          </div>
        </template>
      </NCard>
    </NModal>
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

.settings-subgroup {
  font-size: 12px;
  font-weight: 500;
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.4));
}

.settings-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.5;
}

.form-row {
  display: flex;
  gap: 16px;
  align-items: center;
}

.history-header {
  display: flex;
  align-items: center;
}

.backup-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.backup-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--ashell-hover);
}

.backup-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.backup-name {
  font-size: 13px;
  color: var(--ashell-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.backup-size {
  font-size: 11px;
}

.backup-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.pwd-hint {
  margin: 0 0 12px;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.5;
}

.pwd-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
