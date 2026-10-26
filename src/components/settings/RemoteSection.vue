<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  NAlert,
  NButton,
  NForm,
  NFormItem,
  NInput,
  NSpin,
  NSwitch,
  NTag,
  useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import QRCode from "qrcode";
import {
  cancelWeixinLogin,
  getWeixinStatus,
  pollWeixinLogin,
  saveWeixinConfig,
  startWeixinLogin,
  submitWeixinVerifyCode,
  testWeixinPush,
  weixinLogout,
  type WeixinLoginView,
  type WeixinStatus,
} from "@/api/weixin";

const { t } = useI18n();
const message = useMessage();

const loading = ref(false);
const status = ref<WeixinStatus | null>(null);

// 登录流程状态
const loggingIn = ref(false);
const login = ref<WeixinLoginView | null>(null);
const qrDataUrl = ref("");
const verifyCode = ref("");
const startingLogin = ref(false);
let loginPollTimer: ReturnType<typeof setInterval> | null = null;

const testing = ref(false);
const savingNotify = ref(false);

const loggedIn = computed(() => Boolean(status.value?.logged_in));

function formatTime(ms: number): string {
  if (!ms) return "-";
  return new Date(ms).toLocaleString();
}

async function loadStatus() {
  loading.value = true;
  try {
    status.value = await getWeixinStatus();
  } catch (e) {
    message.error(t("settings.remote.loadFailed", { error: String(e) }));
  } finally {
    loading.value = false;
  }
}

async function renderQr(url: string) {
  try {
    qrDataUrl.value = await QRCode.toDataURL(url, { margin: 1, width: 220 });
  } catch {
    qrDataUrl.value = "";
  }
}

function stopLoginPoll() {
  if (loginPollTimer) {
    clearInterval(loginPollTimer);
    loginPollTimer = null;
  }
}

async function handleLoginState(view: WeixinLoginView) {
  login.value = view;
  if (view.qr_url) {
    await renderQr(view.qr_url);
  }
  if (view.status === "confirmed" || view.status === "failed" || view.status === "binded") {
    stopLoginPoll();
    if (view.status === "confirmed") {
      message.success(t("settings.remote.loginSuccess"));
    }
    loggingIn.value = false;
    await loadStatus();
  }
}

async function handleStartLogin() {
  startingLogin.value = true;
  verifyCode.value = "";
  try {
    const view = await startWeixinLogin();
    loggingIn.value = true;
    await handleLoginState(view);
    stopLoginPoll();
    loginPollTimer = setInterval(async () => {
      try {
        await handleLoginState(await pollWeixinLogin());
      } catch {
        // 网络抖动忽略，下一轮重试
      }
    }, 1500);
  } catch (e) {
    message.error(t("settings.remote.loginStartFailed", { error: String(e) }));
  } finally {
    startingLogin.value = false;
  }
}

async function handleCancelLogin() {
  stopLoginPoll();
  loggingIn.value = false;
  login.value = null;
  qrDataUrl.value = "";
  try {
    await cancelWeixinLogin();
  } catch {
    // ignore
  }
}

async function handleSubmitCode() {
  const code = verifyCode.value.trim();
  if (!code) return;
  try {
    await submitWeixinVerifyCode(code);
    if (login.value) {
      login.value = { ...login.value, status: "scanned", message: "" };
    }
  } catch (e) {
    message.error(String(e));
  }
}

async function handleLogout() {
  try {
    await weixinLogout();
    message.success(t("settings.remote.logoutSuccess"));
    await loadStatus();
  } catch (e) {
    message.error(t("settings.remote.logoutFailed", { error: String(e) }));
  }
}

async function handleNotifyToggle(enabled: boolean) {
  if (!status.value) return;
  savingNotify.value = true;
  try {
    await saveWeixinConfig(enabled);
    status.value.notify_enabled = enabled;
    message.success(t("settings.remote.saveSuccess"));
  } catch (e) {
    status.value.notify_enabled = !enabled;
    message.error(t("settings.remote.saveFailed", { error: String(e) }));
  } finally {
    savingNotify.value = false;
  }
}

async function handleTest() {
  testing.value = true;
  try {
    await testWeixinPush();
    message.success(t("settings.remote.testSuccess"));
  } catch (e) {
    message.error(t("settings.remote.testFailed", { error: String(e) }));
  } finally {
    testing.value = false;
  }
}

onMounted(loadStatus);
onBeforeUnmount(() => {
  stopLoginPoll();
  if (loggingIn.value) {
    void cancelWeixinLogin().catch(() => {});
  }
});
</script>

<template>
  <section class="settings-section">
    <div class="settings-section-title">{{ t("settings.remote.title") }}</div>
    <p class="settings-hint">{{ t("settings.remote.description") }}</p>

    <NSpin :show="loading">
      <!-- 未登录：引导扫码 -->
      <template v-if="!loggedIn && !loggingIn">
        <NAlert type="info" :show-icon="false" class="hint-block">
          {{ t("settings.remote.notLoggedIn") }}
        </NAlert>
        <NButton
          type="primary"
          size="small"
          :loading="startingLogin"
          @click="handleStartLogin"
        >
          {{ t("settings.remote.startLogin") }}
        </NButton>
      </template>

      <!-- 登录中：二维码 + 状态 -->
      <template v-if="loggingIn">
        <div class="login-panel">
          <div v-if="qrDataUrl" class="qr-wrap">
            <img :src="qrDataUrl" :alt="t('settings.remote.qrAlt')" class="qr-img" />
          </div>
          <div class="login-status">
            <template v-if="login?.status === 'need_verifycode'">
              <p class="settings-hint">{{ t("settings.remote.verifyPrompt") }}</p>
              <NForm inline :show-feedback="false">
                <NFormItem>
                  <NInput
                    v-model:value="verifyCode"
                    :placeholder="t('settings.remote.verifyPlaceholder')"
                    style="max-width: 140px"
                    @keyup.enter="handleSubmitCode"
                  />
                </NFormItem>
                <NFormItem>
                  <NButton size="small" type="primary" @click="handleSubmitCode">
                    {{ t("settings.remote.verifySubmit") }}
                  </NButton>
                </NFormItem>
              </NForm>
            </template>
            <template v-else>
              <p class="settings-hint">
                {{
                  login?.message || t(`settings.remote.loginStatus_${login?.status || "waiting"}`)
                }}
              </p>
            </template>
            <NButton size="small" quaternary @click="handleCancelLogin">
              {{ t("settings.remote.cancelLogin") }}
            </NButton>
          </div>
        </div>
      </template>

      <!-- 已登录：状态 + 操作 -->
      <template v-if="loggedIn">
        <div class="status-grid">
          <div class="status-item">
            <span class="status-label">{{ t("settings.remote.accountLabel") }}</span>
            <NTag :type="status?.polling ? 'success' : 'warning'" size="small" :bordered="false">
              {{ status?.polling ? t("settings.remote.stateOnline") : t("settings.remote.stateIdle") }}
            </NTag>
          </div>
          <div class="status-item">
            <span class="status-label">{{ t("settings.remote.botLabel") }}</span>
            <span class="status-value">{{ status?.bot_id || "-" }}</span>
          </div>
          <div class="status-item">
            <span class="status-label">{{ t("settings.remote.loginTimeLabel") }}</span>
            <span class="status-value">{{ formatTime(status?.saved_at || 0) }}</span>
          </div>
          <div class="status-item">
            <span class="status-label">{{ t("settings.remote.peersLabel") }}</span>
            <span class="status-value">{{ status?.peers?.length || 0 }}</span>
          </div>
        </div>

        <NAlert
          v-if="status?.paused_remaining_ms"
          type="warning"
          :show-icon="false"
          class="hint-block"
        >
          {{ t("settings.remote.pausedHint", { minutes: Math.ceil((status.paused_remaining_ms || 0) / 60000) }) }}
        </NAlert>
        <NAlert v-else-if="!status?.peers?.length" type="info" :show-icon="false" class="hint-block">
          {{ t("settings.remote.pairHint") }}
        </NAlert>

        <NForm label-placement="top" require-mark-placement="left">
          <NFormItem :label="t('settings.remote.notifyEnabled')">
            <NSwitch
              :value="status?.notify_enabled"
              :loading="savingNotify"
              @update:value="handleNotifyToggle"
            />
          </NFormItem>
          <p class="settings-hint switch-hint">{{ t("settings.remote.notifyHint") }}</p>
          <NFormItem :show-label="false">
            <div class="form-actions">
              <NButton size="small" type="primary" :loading="testing" @click="handleTest">
                {{ t("settings.remote.testPush") }}
              </NButton>
              <NButton size="small" quaternary type="error" @click="handleLogout">
                {{ t("settings.remote.logout") }}
              </NButton>
            </div>
          </NFormItem>
        </NForm>
      </template>
    </NSpin>
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
  margin: 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #98a2b3);
  line-height: 1.5;
}

.hint-block {
  margin-right: auto;
}

.login-panel {
  display: flex;
  gap: 16px;
  align-items: center;
}

.qr-wrap {
  padding: 8px;
  background: #fff;
  border-radius: 8px;
  line-height: 0;
}

.qr-img {
  width: 200px;
  height: 200px;
}

.login-status {
  display: flex;
  flex-direction: column;
  gap: 10px;
  align-items: flex-start;
}

.status-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 24px;
}

.status-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.status-label {
  color: var(--ashell-text-muted, #98a2b3);
  flex-shrink: 0;
}

.status-value {
  color: var(--ashell-text-strong);
  word-break: break-all;
}

.switch-hint {
  margin-top: -14px;
}

.form-actions {
  display: flex;
  gap: 8px;
}
</style>
