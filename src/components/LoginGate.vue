<script setup lang="ts">
import { onMounted, ref } from "vue";
import { NButton, NIcon, NInput, NCard } from "naive-ui";
import { useI18n } from "vue-i18n";
import { LockClosedOutline } from "@vicons/ionicons5";
import { getStoredToken, setStoredToken } from "@/api/client";
import { isTauri } from "@/utils/platform";

/**
 * Web 形态的登录门（桌面端直接透传内容）。
 *
 * 单用户模型：访问令牌即密码。挂载时若已有令牌则先静默校验；
 * 通过 / 失败清令牌后展示登录表单。登录成功写入令牌并整页刷新，
 * 让所有 store 携带有效令牌重新初始化。
 */
const { t } = useI18n();

const passed = ref(isTauri);
const checking = ref(!isTauri);
const password = ref("");
const submitting = ref(false);
const error = ref("");

async function login(pwd: string): Promise<{ ok: boolean; invalid?: boolean }> {
  try {
    const resp = await fetch("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ password: pwd }),
    });
    if (resp.status === 401) return { ok: false, invalid: true };
    if (!resp.ok) return { ok: false };
    const env = (await resp.json()) as { code?: number; data?: { token?: string } };
    const token = env?.data?.token;
    if (!token) return { ok: false };
    setStoredToken(token);
    return { ok: true };
  } catch {
    return { ok: false };
  }
}

async function verifyStored() {
  const token = getStoredToken();
  if (!token) {
    checking.value = false;
    return;
  }
  const res = await login(token);
  if (res.ok) {
    passed.value = true;
    return;
  }
  // 令牌失效（服务端重启后重置等）：清除回到登录表单
  setStoredToken("");
  checking.value = false;
}

async function submit() {
  const pwd = password.value;
  if (!pwd || submitting.value) return;
  submitting.value = true;
  error.value = "";
  const res = await login(pwd);
  submitting.value = false;
  if (res.ok) {
    // 整页刷新：所有 store 以新令牌重新初始化
    window.location.reload();
    return;
  }
  error.value = res.invalid ? t("auth.invalid") : t("auth.networkError");
}

onMounted(() => {
  if (!isTauri) void verifyStored();
});
</script>

<template>
  <slot v-if="passed" />
  <div v-else class="login-screen">
    <div v-if="checking" class="login-checking">
      <span class="spinner" />
      <p>{{ t("auth.checking") }}</p>
    </div>
    <NCard v-else class="login-card" :bordered="true">
      <div class="login-head">
        <div class="login-logo">
          <img src="/icon.png" alt="AShell" />
        </div>
        <h1 class="login-title">AShell</h1>
        <p class="login-sub">{{ t("auth.subtitle") }}</p>
      </div>
      <form class="login-form" @submit.prevent="submit">
        <NInput
          v-model:value="password"
          type="password"
          show-password-on="click"
          :placeholder="t('auth.placeholder')"
          :disabled="submitting"
          autofocus
          @update:value="error = ''"
        >
          <template #prefix>
            <NIcon :size="16" class="login-lock"><LockClosedOutline /></NIcon>
          </template>
        </NInput>
        <div v-if="error" class="login-error">{{ error }}</div>
        <NButton
          attr-type="submit"
          type="primary"
          block
          :loading="submitting"
          :disabled="!password"
        >
          {{ t("auth.login") }}
        </NButton>
      </form>
      <p class="login-hint">{{ t("auth.hint") }}</p>
    </NCard>
  </div>
</template>

<style scoped>
.login-screen {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--ashell-bg, #14150f);
}

.login-checking {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: var(--ashell-text-muted, #8a8b7a);
}

.spinner {
  width: 28px;
  height: 28px;
  border: 3px solid var(--ashell-border, #3a3b32);
  border-top-color: var(--ashell-accent, #a3be5f);
  border-radius: 50%;
  animation: login-spin 0.8s linear infinite;
}

@keyframes login-spin {
  to {
    transform: rotate(360deg);
  }
}

.login-card {
  width: 340px;
  border-radius: 12px;
}

.login-head {
  text-align: center;
  margin-bottom: 20px;
}

.login-logo {
  width: 56px;
  height: 56px;
  margin: 0 auto 10px;
}

.login-logo img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.login-title {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
  color: var(--ashell-text-strong, #e8e9df);
}

.login-sub {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--ashell-text-muted, #8a8b7a);
}

.login-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.login-lock {
  color: var(--ashell-text-muted, #8a8b7a);
}

.login-error {
  font-size: 12px;
  color: #e05f65;
}

.login-hint {
  margin: 14px 0 0;
  font-size: 11px;
  line-height: 1.6;
  color: var(--ashell-text-muted, #8a8b7a);
  text-align: center;
}
</style>
