<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { marked } from "marked";
import hljs from "highlight.js";
import DOMPurify from "dompurify";
import {
  NInput,
  NButton,
  NIcon,
  NScrollbar,
  NAvatar,
  NSpace,
  NModal,
  NCard,
  NForm,
  NFormItem,
  NSelect,
  NSpin,
} from "naive-ui";
import {
  SendOutline,
  SparklesOutline,
  PersonOutline,
  RefreshOutline,
  CloseOutline,
  SettingsOutline,
  StopCircleOutline,
  BuildOutline,
} from "@vicons/ionicons5";
import { useI18n } from "vue-i18n";
import type { ChatMessage, ProcessStep } from "@/types";
import { useApiStore } from "@/stores/api";
import { useAiStore } from "@/stores/ai";
import { getApiInfo } from "@/api/client";

const props = defineProps<{
  open: boolean;
  /** 当前激活终端 tab 的 SSH session id（无激活终端时为空） */
  sid?: string | null;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
}>();

const apiStore = useApiStore();
const aiStore = useAiStore();
const { t } = useI18n();

// ── Model config ──
//
// 配置存储在 ~/.ashell/ai/.env，由后端 read_ai_env / write_ai_env 命令读写。
// localStorage 不再保留同名缓存，唯一来源就是 .env 文件本身——这样
// sidecar 启动时读到的配置与设置弹窗里看到的一定一致。

type AiModelConfig = {
  url: string;
  key: string;
  modelIds: string;
  activeModelId: string;
  sidecarType: string;
  piProvider: string;
  piModel: string;
  piModelIds: string;
  piBaseUrl: string;
  piApiKey: string;
  piApi: string;
  piThinkingLevel: string;
};

function emptyConfig(): AiModelConfig {
  return {
    url: "", key: "", modelIds: "", activeModelId: "", sidecarType: "",
    piProvider: "", piModel: "", piModelIds: "", piBaseUrl: "", piApiKey: "", piApi: "", piThinkingLevel: "",
  };
}

function parseModelIds(value: string) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function normalizeModelIds(value: string) {
  return parseModelIds(value).join(", ");
}

function resolveActiveModelId(modelIds: string, activeModelId: string) {
  const list = parseModelIds(modelIds);
  return list.includes(activeModelId) ? activeModelId : (list[0] ?? "");
}

const modelConfig = ref<AiModelConfig>(emptyConfig());
const draftConfig = ref<AiModelConfig>(emptyConfig());
const settingsOpen = ref(false);
const settingsLoading = ref(false);
const settingsSaving = ref(false);

const activeModelLabel = computed(() => {
  if (activeSidecarType.value === "pi") {
    return modelConfig.value.piModel || "No model selected";
  }
  return modelConfig.value.activeModelId || "No model selected";
});

const modelOptions = computed(() =>
  parseModelIds(draftConfig.value.modelIds).map((id) => ({
    label: id,
    value: id,
  })),
);

function syncDraftActiveModel() {
  draftConfig.value.activeModelId = resolveActiveModelId(
    draftConfig.value.modelIds,
    draftConfig.value.activeModelId,
  );
}

const sidecarTypeOptions = computed(() => [
  { label: "Claude Agent SDK", value: "claude" },
  { label: "Pi Coding Agent", value: "pi" },
]);

const piApiOptions = computed(() => [
  { label: "OpenAI Completions", value: "openai-completions" },
  { label: "Anthropic Messages", value: "anthropic-messages" },
  { label: "OpenAI Responses", value: "openai-responses" },
  { label: "Google Generative AI", value: "google-generative-ai" },
]);

const piThinkingLevelOptions = computed(() => [
  { label: "Off", value: "off" },
  { label: "Minimal", value: "minimal" },
  { label: "Low", value: "low" },
  { label: "Medium", value: "medium" },
  { label: "High", value: "high" },
  { label: "XHigh", value: "xhigh" },
  { label: "Max", value: "max" },
]);

const isPiDraft = computed(() => draftConfig.value.sidecarType === "pi");

const piModelOptions = computed(() =>
  parseModelIds(draftConfig.value.piModelIds).map((id) => ({
    label: id,
    value: id,
  })),
);

function syncDraftPiModel() {
  draftConfig.value.piModel = resolveActiveModelId(
    draftConfig.value.piModelIds,
    draftConfig.value.piModel,
  );
}

/** sidecar 类型是否与当前运行中的不同（保存后需重启 sidecar） */
const sidecarTypeChanged = computed(
  () => draftConfig.value.sidecarType !== modelConfig.value.sidecarType,
);

/** 当前生效的 sidecar 类型，空值视为 "claude" */
const activeSidecarType = computed(() =>
  modelConfig.value.sidecarType || "claude",
);

async function loadModelConfigFromBackend(): Promise<AiModelConfig> {
  try {
    const raw = await invoke<AiModelConfig>("read_ai_env");
    return {
      url: raw.url ?? "",
      key: raw.key ?? "",
      modelIds: normalizeModelIds(raw.modelIds ?? ""),
      activeModelId: raw.activeModelId ?? "",
      sidecarType: raw.sidecarType ?? "",
      piProvider: raw.piProvider ?? "",
      piModel: raw.piModel ?? "",
      piModelIds: raw.piModelIds ?? "",
      piBaseUrl: raw.piBaseUrl ?? "",
      piApiKey: raw.piApiKey ?? "",
      piApi: raw.piApi ?? "",
      piThinkingLevel: raw.piThinkingLevel ?? "",
    };
  } catch (err) {
    console.error("[AI] read_ai_env failed:", err);
    return emptyConfig();
  }
}

async function openSettings() {
  settingsOpen.value = true;
  settingsLoading.value = true;
  try {
    const cfg = await loadModelConfigFromBackend();
    modelConfig.value = cfg;
    draftConfig.value = { ...cfg };
  } finally {
    settingsLoading.value = false;
  }
}

async function saveSettings() {
  if (settingsSaving.value) return;
  const modelIds = normalizeModelIds(draftConfig.value.modelIds);
  const next: AiModelConfig = {
    url: draftConfig.value.url.trim(),
    key: draftConfig.value.key.trim(),
    modelIds,
    activeModelId: resolveActiveModelId(modelIds, draftConfig.value.activeModelId),
    sidecarType: draftConfig.value.sidecarType.trim(),
    piProvider: draftConfig.value.piProvider.trim(),
    piModel: draftConfig.value.piModel.trim(),
    piModelIds: draftConfig.value.piModelIds.trim(),
    piBaseUrl: draftConfig.value.piBaseUrl.trim(),
    piApiKey: draftConfig.value.piApiKey.trim(),
    piApi: draftConfig.value.piApi.trim(),
    piThinkingLevel: draftConfig.value.piThinkingLevel.trim(),
  };

  settingsSaving.value = true;
  try {
    await invoke("write_ai_env", { config: next });
    modelConfig.value = next;
    settingsOpen.value = false;
  } catch (err) {
    console.error("[AI] write_ai_env failed:", err);
  } finally {
    settingsSaving.value = false;
  }
}

// ── Panel resize ──

const MIN_WIDTH = 320;
const DEFAULT_WIDTH = 420;
// Drag cap: 90% of viewport width so the panel never fully covers the main view
function getMaxWidth(): number {
  return Math.round(window.innerWidth * 0.9);
}
const WIDTH_KEY = "ashell:ai-width";

function loadWidth(): number {
  const raw =
    typeof localStorage !== "undefined"
      ? localStorage.getItem(WIDTH_KEY)
      : null;
  const n = raw ? Number(raw) : NaN;
  if (!Number.isFinite(n)) return DEFAULT_WIDTH;
  return Math.min(getMaxWidth(), Math.max(MIN_WIDTH, n));
}

function saveWidth(v: number) {
  try {
    localStorage.setItem(WIDTH_KEY, String(v));
  } catch {
    // ignore
  }
}

const width = ref<number>(loadWidth());
const resizing = ref(false);

function onResizeStart(e: PointerEvent) {
  e.preventDefault();
  resizing.value = true;
  window.addEventListener("pointermove", onResizeMove);
  window.addEventListener("pointerup", onResizeEnd);
  window.addEventListener("pointercancel", onResizeEnd);
}

function onResizeMove(e: PointerEvent) {
  const next = Math.round(window.innerWidth - e.clientX);
  width.value = Math.min(getMaxWidth(), Math.max(MIN_WIDTH, next));
}

function onResizeEnd() {
  if (!resizing.value) return;
  resizing.value = false;
  saveWidth(width.value);
  window.removeEventListener("pointermove", onResizeMove);
  window.removeEventListener("pointerup", onResizeEnd);
  window.removeEventListener("pointercancel", onResizeEnd);
}

onBeforeUnmount(onResizeEnd);

const panelStyle = computed(() => ({
  width: `${width.value}px`,
  transition: resizing.value ? "none" : "transform 0.25s ease",
  transform: props.open ? "translateX(0)" : "translateX(100%)",
}));

// ── Markdown ──

const DOMPURIFY_CONFIG = {
  ALLOWED_TAGS: [
    "p", "br", "hr", "pre", "blockquote", "code", "kbd", "samp", "var",
    "h1", "h2", "h3", "h4", "h5", "h6",
    "strong", "b", "em", "i", "u", "s", "strike", "del", "ins", "mark",
    "small", "sub", "sup",
    "ul", "ol", "li", "dl", "dt", "dd",
    "a", "img", "figure", "figcaption",
    "table", "thead", "tbody", "tfoot", "tr", "th", "td", "caption",
    "div", "span", "section", "article", "aside", "header", "footer", "nav", "main",
    "details", "summary",
    "abbr", "address", "cite", "q", "time", "data", "wbr",
    "input",
  ],
  ALLOWED_ATTR: [
    "href", "src", "alt", "title", "class", "id", "name", "target", "rel",
    "width", "height", "colspan", "rowspan", "type", "aria-*", "data-*",
    "checked",
  ],
  FORBID_TAGS: ["script", "style", "iframe", "form", "button", "svg", "math"],
  FORBID_ATTR: ["onerror", "onload", "onmouseover", "onfocus", "onblur", "onclick"],
  ADD_TAGS: ["custom-tag"],
  ADD_ATTR: ["data-custom"],
};

marked.setOptions({ breaks: true, gfm: true });

const codeHighlightExtension = {
  name: "code",
  renderer(token: any) {
    const text = token.text || "";
    const lang = token.lang || "";
    const language = lang && hljs.getLanguage(lang) ? lang : "plaintext";
    const highlighted = hljs.highlight(text, { language }).value;
    const langLabel =
      language !== "plaintext"
        ? `<div class="code-lang">${language}</div>`
        : "";
    return `${langLabel}<pre><code class="hljs language-${language}">${highlighted}</code></pre>`;
  },
};

marked.use({ extensions: [codeHighlightExtension] });

function escapeHtml(text: string): string {
  if (!text) return "";
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

function parseMarkdown(content: string): string {
  return DOMPurify.sanitize(
    marked.parse(content) as string,
    DOMPURIFY_CONFIG,
  );
}

// ── Chat state ──
//
// 状态分两类：
// - 全局 UI 状态（input、settingsOpen、showConfirmDialog 等）保留在组件局部
// - 每个 ssid 的会话状态（messages/seq/isTyping/...）从 useAiStore 取
//
// 当 props.sid 为空（无激活终端）时，组件展示 empty state，不允许 spawn 也不允许 send。

const ASSISTANT_NAME = computed(() => t("ai.name"));

const input = ref("");
const showConfirmDialog = ref(false);
const scrollbar = ref<InstanceType<typeof NScrollbar> | null>(null);

/** 当前激活的 ssid（空字符串表示无终端） */
const currentSsid = computed(() => props.sid ?? "");

/** 当前 ssid 的会话；ssid 为空时返回 undefined */
const currentSession = computed(() =>
  currentSsid.value ? aiStore.sessions[currentSsid.value] : undefined,
);

const messages = computed<ChatMessage[]>(
  () => currentSession.value?.messages ?? [],
);
const isTyping = computed(() => currentSession.value?.isTyping ?? false);
const isApprovalActive = computed(
  () => currentSession.value?.isApprovalActive ?? false,
);

const canSend = computed(
  () =>
    input.value.trim().length > 0 &&
    !isTyping.value &&
    currentSsid.value !== "" &&
    currentSession.value?.sidecarPid !== null &&
    currentSession.value?.sidecarPid !== undefined,
);

function nowStr() {
  const d = new Date();
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** 过程块折叠态文案：工具调用次数 + 最近一次工具名 */
function processSummary(steps: ProcessStep[] | undefined): string {
  if (!steps || steps.length === 0) return t("ai.process.empty");
  const toolCalls = steps.filter((s) => s.type === "tool_call");
  const count = toolCalls.length;
  const lastName = toolCalls[toolCalls.length - 1]?.toolName;
  if (count === 0) return t("ai.process.onlyReturns", { n: steps.length });
  if (lastName) return t("ai.process.summary", { count, name: lastName });
  return t("ai.process.count", { count });
}

function scrollBottom() {
  nextTick(() => scrollbar.value?.scrollTo({ top: 1e9, behavior: "smooth" }));
}

function close() {
  emit("update:open", false);
}

// ── Sidecar ──

let unlistenApiMessage: UnlistenFn | null = null;

/** 取裸 token（不含 "Bearer " 前缀）；apiStore 未就绪时主动 fetch 一次 */
async function getToken(): Promise<string> {
  if (apiStore.info?.token) return apiStore.info.token;
  try {
    const info = await getApiInfo();
    return info.token;
  } catch {
    return "";
  }
}

/** 取本机 axum API 监听地址（形如 127.0.0.1:53421）；apiStore 未就绪时主动 fetch 一次 */
async function getApiAddr(): Promise<string> {
  if (apiStore.info?.addr) return apiStore.info.addr;
  try {
    const info = await getApiInfo();
    return info.addr;
  } catch {
    return "";
  }
}

/**
 * 确保指定 ssid 的 sidecar 已启动并准备好接收消息。
 * - 已存在且 PID 有效 → 直接返回（保持现有会话历史）
 * - 不存在 → spawn 新进程并 push 欢迎消息
 */
async function ensureSidecarFor(ssid: string) {
  if (!ssid) return;

  const existing = aiStore.sessions[ssid];
  if (existing && existing.sidecarPid !== null) {
    return;
  }

  // 占位 session，避免并发 spawn
  aiStore.ensure(ssid);

  const workspace = await invoke<string>("get_ai_dir");
  const token = await getToken();
  const addr = await getApiAddr();

  const pid = await aiStore.spawnFor(
    ssid,
    { workspace, token, addr, sidecarType: activeSidecarType.value },
    (line) => handleSidecarOutput(ssid, line),
  );

  if (pid !== null) {
    aiStore.pushMessage(ssid, {
      role: "assistant",
      content: t("ai.welcome", { name: ASSISTANT_NAME.value }),
      time: nowStr(),
    });
    scrollBottom();
  }
}

function handleSidecarOutput(ssid: string, line: string) {
  if (line.startsWith("[END_OF_RESPONSE]")) {
    const session = aiStore.sessions[ssid];
    if (session) {
      const streamingMsg = session.messages.find((m) => m.isStreaming);
      if (streamingMsg) {
        aiStore.updateMessage(ssid, streamingMsg.id, { isStreaming: false });
      }
    }
    aiStore.finalizeProcess(ssid);
    aiStore.patch(ssid, { isTyping: false, isApprovalActive: false });
    return;
  }

  if (line.startsWith("[SESSION_STOP]")) {
    const aiMsg = line.substring("[SESSION_STOP]".length).trim();
    const session = aiStore.sessions[ssid];
    if (session) {
      const streamingMsg = session.messages.find((m) => m.isStreaming);
      if (streamingMsg) {
        aiStore.updateMessage(ssid, streamingMsg.id, { isStreaming: false });
      }
    }
    aiStore.finalizeProcess(ssid);
    aiStore.patch(ssid, {
      isTyping: false,
      isSessionActive: false,
      isApprovalActive: false,
    });
    if (aiMsg) {
      aiStore.pushMessage(ssid, {
        role: "assistant",
        content: aiMsg,
        time: nowStr(),
      });
    }
    return;
  }

  if (line.startsWith("[STOPPED]")) {
    const session = aiStore.sessions[ssid];
    if (session) {
      const streamingMsg = session.messages.find((m) => m.isStreaming);
      if (streamingMsg) {
        aiStore.updateMessage(ssid, streamingMsg.id, { isStreaming: false });
      }
    }
    aiStore.finalizeProcess(ssid);
    aiStore.patch(ssid, { isTyping: false, isApprovalActive: false });
    pushAssistantMessage(ssid, t("ai.stopped"));
    return;
  }

  if (line.startsWith("[SYSTEM_API_RETRY]")) {
    const aiMsg = line.substring("[SYSTEM_API_RETRY]".length);
    const msg = JSON.parse(aiMsg);
    const content = `**[error]** ${escapeHtml(msg.payload)}`;
    aiStore.finalizeProcess(ssid);
    pushAssistantMessage(ssid, content);
    return;
  }

  if (line.startsWith("[AIMSG]")) {
    const aiMsg = line.substring("[AIMSG]".length);
    const msg = JSON.parse(aiMsg);
    aiStore.finalizeProcess(ssid);
    pushAssistantMessage(ssid, msg.payload);
    return;
  }

  if (line.startsWith("[AI_THINKING]")) {
    const aiMsg = line.substring("[AI_THINKING]".length);
    const msg = JSON.parse(aiMsg);
    aiStore.finalizeProcess(ssid);
    pushAssistantMessage(ssid, msg.payload, true);
    return;
  }

  if (line.startsWith("[AITOOL]")) {
    const aiMsg = line.substring("[AITOOL]".length);
    const aiToolMsg = JSON.parse(aiMsg);
    const tool = aiToolMsg.payload;

    let content = "";
    if (tool.command === undefined) {
      content = `> Tool Call **[${tool.name}]**\n`;
    } else {
      content = `${tool.description}\n> Tool Call **[${tool.name}]**\n**Command:**\n\`\`\`\n${tool.command}\n\`\`\`\n`;
    }

    const step: ProcessStep = {
      type: "tool_call",
      content,
      toolName: String(tool.name ?? ""),
      time: nowStr(),
    };
    aiStore.appendProcessStep(ssid, step);
    if (ssid === currentSsid.value) scrollBottom();
    return;
  }

  if (line.startsWith("[TOOL_RET]")) {
    const aiMsg = line.substring("[TOOL_RET]".length);
    const msg = JSON.parse(aiMsg);
    const payload = msg.payload;
    let content = "";
    for (const item of payload as any[]) {
      content += `**[${item.type}]**\n<details><summary>${t("ai.toolRetDetail")}</summary>\n\n\`\`\`json\n${JSON.stringify(item.content, null, 2)}\n\`\`\`\n</details>\n\n`;
    }
    const step: ProcessStep = {
      type: "tool_ret",
      content,
      time: nowStr(),
    };
    aiStore.appendProcessStep(ssid, step);
    if (ssid === currentSsid.value) scrollBottom();
    return;
  }

  if (line.startsWith("[TOOL_CONFIRM]")) {
    const aiMsg = line.substring("[TOOL_CONFIRM]".length);
    const msg = JSON.parse(aiMsg);
    const obj = msg.payload;

    let content = "";
    if (obj.options) {
      content += `**Action:**\n\n`;
      for (const [key, value] of Object.entries(obj.options as Record<string, unknown>)) {
        if (key === "command") {
          content += `\`\`\`\n${value}\n\`\`\`\n`;
        } else {
          content += `- **${key}:** ${value}\n`;
        }
      }
    }
    content += `\n**${obj.question.trim()}**`;

    aiStore.finalizeProcess(ssid);
    aiStore.patch(ssid, { isApprovalActive: true });
    pushAssistantMessage(ssid, content);
    return;
  }

  if (line.startsWith("[AI_ASKUSERQUESTION]")) {
    const aiMsg = line.substring("[AI_ASKUSERQUESTION]".length);
    const msg = JSON.parse(aiMsg);
    const payload = msg.payload;

    const items = payload.items.join("\n");
    const content = `**${payload.title}**\n\n${items}\n\n${payload.tips}`;
    aiStore.finalizeProcess(ssid);
    aiStore.patch(ssid, { isTyping: false });
    pushAssistantMessage(ssid, content);
    return;
  }
}

function pushAssistantMessage(ssid: string, content: string, thinking = false) {
  aiStore.pushMessage(ssid, {
    role: "assistant",
    content,
    time: nowStr(),
    thinking,
  });
  if (ssid === currentSsid.value) scrollBottom();
}

/** 兼容旧的 defineExpose 接口：向当前 ssid 推送一条 assistant 消息 */
function callStreamingApi(content: string) {
  const ssid = currentSsid.value;
  if (!ssid) return;
  pushAssistantMessage(ssid, content);
}

/**
 * 外部调用：向当前 ssid 的 AI 会话发送一条用户消息。
 * 确保 sidecar 已启动，推送 user message 并写入 stdin。
 */
async function sendText(text: string) {
  const ssid = currentSsid.value;
  if (!ssid || !text.trim()) return;
  await ensureSidecarFor(ssid);
  const session = aiStore.sessions[ssid];
  if (!session || session.sidecarPid === null) return;
  if (session.isTyping) return;

  aiStore.pushMessage(ssid, {
    role: "user",
    content: text,
    time: nowStr(),
  });
  aiStore.patch(ssid, { isTyping: true });

  const formattedContent = JSON.stringify(text).trim().slice(1, -1);
  await aiStore.writeTo(ssid, formattedContent + "\n");
  scrollBottom();
}

async function sendMessage() {
  const content = input.value.trim();
  const ssid = currentSsid.value;
  if (!content || isTyping.value || !ssid) return;
  const session = aiStore.sessions[ssid];
  if (!session || session.sidecarPid === null) return;

  aiStore.pushMessage(ssid, {
    role: "user",
    content,
    time: nowStr(),
  });
  input.value = "";
  aiStore.patch(ssid, { isTyping: true });

  const formattedContent = JSON.stringify(content).trim().slice(1, -1);
  await aiStore.writeTo(ssid, formattedContent + "\n");
  scrollBottom();
}

function handleKeyPress(event: KeyboardEvent) {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    sendMessage();
  }
}

async function handleStopSession() {
  const ssid = currentSsid.value;
  if (!ssid) return;
  const session = aiStore.sessions[ssid];
  if (!session || session.sidecarPid === null) return;

  await aiStore.writeTo(ssid, "__STOP__\n");
  const streamingMsg = session.messages.find((m) => m.isStreaming);
  if (streamingMsg) {
    aiStore.updateMessage(ssid, streamingMsg.id, { isStreaming: false });
  }
  aiStore.patch(ssid, { isTyping: false, isApprovalActive: false });
}

async function handleApprovalYes() {
  await sendApproval("y");
}

async function handleApprovalNo() {
  await sendApproval("n");
}

async function sendApproval(content: "y" | "n") {
  const ssid = currentSsid.value;
  if (!ssid) return;
  const session = aiStore.sessions[ssid];
  if (!session || session.sidecarPid === null) return;

  aiStore.pushMessage(ssid, {
    role: "user",
    content,
    time: nowStr(),
  });
  await aiStore.writeTo(ssid, content + "\n");
  aiStore.patch(ssid, { isTyping: true, isApprovalActive: false });
  scrollBottom();
}

function handleNewChat() {
  if (messages.value.length === 0) return;
  showConfirmDialog.value = true;
}

async function confirmNewChat() {
  showConfirmDialog.value = false;
  const ssid = currentSsid.value;
  if (!ssid) return;

  // kill 旧进程并清空对话
  await aiStore.killFor(ssid);

  // 重新 spawn（ensureSidecarFor 会推送欢迎消息）
  await ensureSidecarFor(ssid);
}

function cancelNewChat() {
  showConfirmDialog.value = false;
}

// ── External link handler ──

function onLinkClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  const link = target.closest("a");
  if (link && link.href) {
    const href = link.href;
    const isExternal = !href.startsWith(window.location.origin);
    if (isExternal) {
      e.preventDefault();
      openUrl(href).catch((err) => console.error(t("common.openLinkFailed"), err));
    }
  }
}

// ── Lifecycle ──

onMounted(async () => {
  // 启动时读一次 .env，让 header "Model: xxx" 立即显示当前活动模型
  modelConfig.value = await loadModelConfigFromBackend();

  unlistenApiMessage = await listen<string>("api-message", (event) => {
    const ssid = currentSsid.value;
    if (!ssid) return;
    const session = aiStore.sessions[ssid];
    if (!session || session.sidecarPid === null) return;
    const msg = t("ai.notification", { msg: event.payload });
    pushAssistantMessage(ssid, msg);
  });

  if (currentSsid.value) {
    await ensureSidecarFor(currentSsid.value);
  }
});

// 监听 sid 变化：仅在切到新的 ssid 且尚未 spawn 时启动新 sidecar；
// 切回旧 ssid 不重启进程，会话历史与状态由 store 保留。
watch(
  () => props.sid,
  async (newSid) => {
    if (!newSid) return;
    await ensureSidecarFor(newSid);
  },
);

onBeforeUnmount(() => {
  if (unlistenApiMessage) {
    unlistenApiMessage();
    unlistenApiMessage = null;
  }
  // 注意：不在这里 kill 各 ssid 的 sidecar；
  // sidecar 生命周期跟随对应的 SSH 终端会话，由 App.vue::onStatusChange 在
  // status 变为 closed/error 时统一回收。
});

defineExpose({
  callStreamingApi,
  sendText,
});
</script>

<template>
  <Teleport to="body">
    <aside
      class="ai-panel"
      :class="{ open: props.open, resizing: resizing }"
      :style="panelStyle"
      :aria-hidden="!props.open"
    >
      <div
        class="resize-handle"
        title="Drag to resize"
        @pointerdown="onResizeStart"
      />

      <header class="panel-header">
        <NSpace align="center" :size="10">
          <div class="ai-avatar">
            <NIcon :size="16"><SparklesOutline /></NIcon>
          </div>
          <div>
            <div class="title">{{ t("ai.title") }}</div>
            <div class="subtitle">{{ t("ai.modelLabel", { model: activeModelLabel }) }}</div>
          </div>
        </NSpace>
        <NSpace :size="4">
          <NButton
            quaternary
            circle
            size="small"
            :title="t('ai.restart')"
            @click="handleNewChat"
          >
            <template #icon>
              <NIcon><RefreshOutline /></NIcon>
            </template>
          </NButton>
          <NButton
            quaternary
            circle
            size="small"
            :title="t('ai.settings')"
            @click="openSettings"
          >
            <template #icon>
              <NIcon><SettingsOutline /></NIcon>
            </template>
          </NButton>
          <NButton quaternary circle size="small" :title="t('ai.close')" @click="close">
            <template #icon>
              <NIcon><CloseOutline /></NIcon>
            </template>
          </NButton>
        </NSpace>
      </header>

      <div class="chat-wrap">
        <NScrollbar ref="scrollbar" class="chat-scroll">
          <div class="chat-list" @click="onLinkClick">
            <div
              v-if="!currentSsid"
              class="empty"
            >
              {{ t("ai.needSession") }}
            </div>
            <div
              v-else-if="messages.length === 0"
              class="empty"
            >
              {{ t("ai.placeholder") }}
            </div>
            <div
              v-for="m in messages"
              :key="m.id"
              class="msg"
              :class="m.role"
            >
              <NAvatar
                v-if="m.role === 'assistant'"
                round
                size="small"
                class="bubble-avatar assistant-avatar"
              >
                <NIcon><SparklesOutline /></NIcon>
              </NAvatar>
              <div class="bubble" :class="{ streaming: m.isStreaming }">
                <template v-if="m.isProcess && m.processSteps">
                  <details class="process-block">
                    <summary class="process-summary">
                      <NIcon :size="12" class="process-icon"><BuildOutline /></NIcon>
                      <span class="process-summary-text">{{ processSummary(m.processSteps) }}</span>
                      <span class="process-count">{{ m.processSteps.length }}</span>
                    </summary>
                    <div class="process-steps">
                      <div
                        v-for="(step, i) in m.processSteps"
                        :key="i"
                        class="process-step"
                        :class="step.type === 'tool_call' ? 'step-call' : 'step-ret'"
                        v-html="parseMarkdown(step.content)"
                      />
                    </div>
                  </details>
                </template>
                <template v-else-if="m.thinking">
                  <details class="thinking-block">
                    <summary class="thinking-summary">
                      <span>{{ t("ai.thinking") }}</span>
                    </summary>
                    <div
                      class="thinking-content markdown-body"
                      v-html="parseMarkdown(m.content)"
                    />
                  </details>
                </template>
                <template v-else-if="m.role === 'assistant' && m.content">
                  <div
                    class="markdown-body"
                    v-html="parseMarkdown(m.content)"
                  />
                </template>
                <template v-else-if="m.content">
                  <div class="bubble-content">{{ m.content }}</div>
                </template>
                <span v-else class="dot">…</span>
                <div v-if="!m.isProcess" class="bubble-time">{{ m.time }}</div>
              </div>
              <NAvatar
                v-if="m.role === 'user'"
                round
                size="small"
                class="bubble-avatar user-avatar"
              >
                <NIcon><PersonOutline /></NIcon>
              </NAvatar>
            </div>

            <div
              v-if="isTyping && !messages.some((m) => m.isStreaming)"
              class="msg assistant"
            >
              <NAvatar round size="small" class="bubble-avatar assistant-avatar">
                <NIcon><SparklesOutline /></NIcon>
              </NAvatar>
              <div class="bubble typing">
                <span class="dot" /><span class="dot" /><span class="dot" />
              </div>
            </div>
          </div>
        </NScrollbar>

        <div v-if="isApprovalActive" class="approval-bar">
          <NButton size="small" type="success" @click="handleApprovalYes">
            {{ t("ai.approval.allow") }}
          </NButton>
          <NButton size="small" @click="handleApprovalNo">
            {{ t("ai.approval.deny") }}
          </NButton>
        </div>

        <div
          class="composer-meta"
          :class="{ 'no-border': isApprovalActive }"
          @click="openSettings"
        >
          <NIcon :size="12"><SparklesOutline /></NIcon>
          <span class="composer-meta-label">{{ activeModelLabel }}</span>
        </div>

        <div class="composer">
          <NInput
            v-model:value="input"
            type="textarea"
            :placeholder="t('ai.inputPlaceholder')"
            :autosize="{ minRows: 1, maxRows: 4 }"
            :disabled="isTyping"
            @keydown="handleKeyPress"
          />
          <NButton
            v-if="!isTyping"
            type="primary"
            circle
            :disabled="!canSend"
            @click="sendMessage"
          >
            <template #icon>
              <NIcon><SendOutline /></NIcon>
            </template>
          </NButton>
          <NButton
            v-else
            type="error"
            circle
            @click="handleStopSession"
          >
            <template #icon>
              <NIcon><StopCircleOutline /></NIcon>
            </template>
          </NButton>
        </div>
      </div>

      <!-- Restart session confirm -->
      <NModal v-model:show="showConfirmDialog">
        <NCard
          style="width: min(360px, 80vw)"
          :title="t('ai.restartDialog.title')"
          :bordered="false"
          role="dialog"
          aria-modal="true"
        >
          <p>{{ t("ai.restartDialog.content") }}</p>
          <template #footer>
            <NSpace justify="end">
              <NButton @click="cancelNewChat">{{ t("ai.restartDialog.cancel") }}</NButton>
              <NButton type="primary" @click="confirmNewChat">{{ t("ai.restartDialog.confirm") }}</NButton>
            </NSpace>
          </template>
        </NCard>
      </NModal>
    </aside>

    <!-- Model settings -->
    <NModal v-model:show="settingsOpen">
      <NCard
        style="width: min(460px, 85vw)"
        :title="t('ai.modelDialog.title')"
        size="small"
        :bordered="false"
        role="dialog"
        aria-modal="true"
      >
        <NSpin :show="settingsLoading">
          <NForm label-placement="top" :model="draftConfig">
            <NFormItem :label="t('ai.modelDialog.sidecarType')">
              <NSelect
                v-model:value="draftConfig.sidecarType"
                :options="sidecarTypeOptions"
                :placeholder="t('ai.modelDialog.sidecarTypePlaceholder')"
              />
            </NFormItem>

            <template v-if="!isPiDraft">
              <NFormItem :label="t('ai.modelDialog.baseUrl')">
                <NInput
                  v-model:value="draftConfig.url"
                  placeholder="https://api.anthropic.com"
                />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.authToken')">
                <NInput
                  v-model:value="draftConfig.key"
                  type="password"
                  show-password-on="click"
                  placeholder="sk-..."
                />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.modelIds')">
                <NInput
                  v-model:value="draftConfig.modelIds"
                  type="textarea"
                  :autosize="{ minRows: 2, maxRows: 4 }"
                  placeholder="claude-sonnet-4-5, claude-opus-4-5"
                  @update:value="syncDraftActiveModel"
                />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.activeModel')">
                <NSelect
                  v-model:value="draftConfig.activeModelId"
                  :options="modelOptions"
                  :disabled="modelOptions.length === 0"
                  :placeholder="t('ai.modelDialog.activeModelPlaceholder')"
                />
              </NFormItem>
            </template>

            <template v-else>
              <NFormItem :label="t('ai.modelDialog.piApi')">
                <NSelect
                  v-model:value="draftConfig.piApi"
                  :options="piApiOptions"
                  :placeholder="t('ai.modelDialog.piApiPlaceholder')"
                />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.piProvider')">
                <NInput v-model:value="draftConfig.piProvider" placeholder="custom" />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.piBaseUrl')">
                <NInput v-model:value="draftConfig.piBaseUrl" placeholder="https://api.openai.com/v1" />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.piApiKey')">
                <NInput
                  v-model:value="draftConfig.piApiKey"
                  type="password"
                  show-password-on="click"
                  placeholder="sk-..."
                />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.piModelIds')">
                <NInput
                  v-model:value="draftConfig.piModelIds"
                  type="textarea"
                  :autosize="{ minRows: 2, maxRows: 4 }"
                  placeholder="deepseek-chat, deepseek-coder"
                  @update:value="syncDraftPiModel"
                />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.piActiveModel')">
                <NSelect
                  v-model:value="draftConfig.piModel"
                  :options="piModelOptions"
                  :disabled="piModelOptions.length === 0"
                  :placeholder="t('ai.modelDialog.piActiveModelPlaceholder')"
                />
              </NFormItem>
              <NFormItem :label="t('ai.modelDialog.piThinkingLevel')">
                <NSelect
                  v-model:value="draftConfig.piThinkingLevel"
                  :options="piThinkingLevelOptions"
                  :placeholder="t('ai.modelDialog.piThinkingLevelPlaceholder')"
                />
              </NFormItem>
            </template>

            <div v-if="sidecarTypeChanged" class="settings-hint">
              {{ t("ai.modelDialog.restartHint") }}
            </div>
          </NForm>
        </NSpin>
        <template #footer>
          <NSpace justify="end">
            <NButton :disabled="settingsSaving" @click="settingsOpen = false">
              {{ t("ai.modelDialog.cancel") }}
            </NButton>
            <NButton
              type="primary"
              :loading="settingsSaving"
              :disabled="settingsLoading"
              @click="saveSettings"
            >
              {{ t("ai.modelDialog.save") }}
            </NButton>
          </NSpace>
        </template>
      </NCard>
    </NModal>
  </Teleport>
</template>

<style scoped>
.ai-panel {
  position: fixed;
  top: var(--ashell-header-h);
  right: var(--ashell-activity-w, 0px);
  bottom: 0;
  background: var(--ashell-panel-bg);
  border-left: 1px solid var(--ashell-border);
  box-shadow: -8px 0 24px var(--ashell-shadow);
  display: flex;
  flex-direction: column;
  z-index: 1000;
  user-select: text;
}

.ai-panel.resizing {
  user-select: none;
}

.resize-handle {
  position: absolute;
  top: 0;
  left: -3px;
  width: 6px;
  height: 100%;
  cursor: col-resize;
  z-index: 1;
  background: transparent;
  transition: background 0.15s ease;
}

.resize-handle:hover,
.ai-panel.resizing .resize-handle {
  background: rgba(124, 92, 255, 0.45);
}

/* ── Header ── */

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px;
  border-bottom: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

.ai-avatar {
  width: 32px;
  height: 32px;
  border-radius: 10px;
  background: linear-gradient(
    135deg,
    var(--ashell-primary) 0%,
    #4a8cff 100%
  );
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  box-shadow: 0 4px 12px rgba(124, 92, 255, 0.4);
}

.title {
  font-size: 14px;
  font-weight: 600;
  color: var(--ashell-text-strong);
  line-height: 1.2;
}

.subtitle {
  font-size: 11px;
  color: var(--ashell-text-subtle);
}

/* ── Chat body ── */

.chat-wrap {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.chat-scroll {
  flex: 1;
  min-height: 0;
}

.chat-list {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.empty {
  color: var(--ashell-text-subtle);
  font-size: 13px;
  text-align: center;
  margin-top: 24px;
}

.msg {
  display: flex;
  gap: 8px;
  align-items: flex-end;
  max-width: 100%;
}

.msg.assistant {
  justify-content: flex-start;
}

.msg.user {
  justify-content: flex-end;
}

.bubble {
  max-width: 85%;
  background: var(--ashell-panel-bg-soft);
  border-radius: 12px;
  padding: 10px 12px;
  font-size: 13.5px;
  line-height: 1.55;
  white-space: normal;
  word-break: break-word;
  position: relative;
}

.msg.user .bubble {
  background: linear-gradient(135deg, #7c5cff 0%, #6a4ae6 100%);
  color: #fff;
  white-space: pre-wrap;
}

.bubble-content {
  color: var(--ashell-text);
  white-space: pre-wrap;
}

.msg.user .bubble-content {
  color: #fff;
}

.bubble-time {
  font-size: 10px;
  color: var(--ashell-text-subtle);
  margin-top: 4px;
  text-align: right;
}

/* ── Process block (AITOOL / TOOL_RET 聚合) ── */

.process-block {
  border: 1px solid var(--ashell-border);
  border-radius: 8px;
  background: var(--ashell-terminal-bg);
  overflow: hidden;
}

.process-summary {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  cursor: pointer;
  user-select: none;
  font-size: 12px;
  color: var(--ashell-text-muted);
  list-style: none;
  transition: background 0.15s ease, color 0.15s ease;
}

.process-summary::-webkit-details-marker {
  display: none;
}

.process-summary:hover {
  background: color-mix(in srgb, var(--ashell-primary) 14%, transparent);
  color: var(--ashell-text-strong);
}

.process-icon {
  flex-shrink: 0;
  color: var(--ashell-primary);
}

.process-summary-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.process-count {
  flex-shrink: 0;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--ashell-primary) 22%, transparent);
  color: var(--ashell-primary);
  font-weight: 600;
}

.process-steps {
  padding: 6px 10px 8px;
  border-top: 1px solid var(--ashell-border-soft);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.process-step {
  font-size: 12px;
  line-height: 1.5;
  padding: 6px 8px;
  border-radius: 4px;
  border: 1px solid var(--ashell-border-soft);
  border-left-width: 3px;
}

.process-step.step-call {
  border-left-color: var(--ashell-primary);
  background: color-mix(in srgb, var(--ashell-primary) 12%, transparent);
}

.process-step.step-ret {
  border-left-color: var(--ashell-prompt);
  background: color-mix(in srgb, var(--ashell-prompt) 12%, transparent);
}

.process-step :deep(p) {
  margin: 0 0 4px;
}

.process-step :deep(p:last-child) {
  margin-bottom: 0;
}

.process-step :deep(blockquote) {
  margin: 0 0 4px;
  padding: 2px 8px;
  border-left: 2px solid var(--ashell-primary);
  background: color-mix(in srgb, var(--ashell-primary) 18%, transparent);
  border-radius: 0 3px 3px 0;
  font-size: 11px;
  color: var(--ashell-text);
  display: inline-block;
}

.process-step :deep(blockquote p) {
  margin: 0;
}

.process-step :deep(strong) {
  color: var(--ashell-text-strong);
  font-weight: 600;
}

.process-step :deep(pre) {
  margin: 4px 0;
  padding: 6px 8px;
  border-radius: 4px;
  background: var(--ashell-terminal-bg);
  border: 1px solid var(--ashell-border-soft);
  overflow-x: auto;
  font-size: 11px;
}

.process-step :deep(code) {
  font-family: var(--ashell-mono, "Fira Code", "JetBrains Mono", Menlo, Consolas, monospace);
  font-size: 11px;
}

.process-step :deep(:not(pre) > code) {
  padding: 1px 4px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--ashell-primary) 20%, transparent);
}

.process-step :deep(details) {
  margin: 2px 0;
  font-size: 11px;
}

.process-step :deep(summary) {
  cursor: pointer;
  color: var(--ashell-text-subtle);
  font-size: 11px;
  padding: 2px 0;
}

.process-step :deep(details > pre) {
  margin-top: 4px;
}

/* ── Thinking block ── */

.thinking-block {
  border: 1px solid var(--ashell-border-soft);
  border-radius: 8px;
  background: color-mix(in srgb, var(--ashell-primary) 5%, transparent);
  overflow: hidden;
}

.thinking-summary {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  cursor: pointer;
  user-select: none;
  font-size: 12px;
  color: var(--ashell-text-muted);
  list-style: none;
  transition: background 0.15s ease, color 0.15s ease;
}

.thinking-summary::-webkit-details-marker {
  display: none;
}

.thinking-summary::before {
  content: "▶";
  font-size: 9px;
  transition: transform 0.15s ease;
}

.thinking-block[open] .thinking-summary::before {
  transform: rotate(90deg);
}

.thinking-summary:hover {
  background: color-mix(in srgb, var(--ashell-primary) 10%, transparent);
  color: var(--ashell-text-strong);
}

.thinking-content {
  padding: 8px 10px;
  border-top: 1px solid var(--ashell-border-soft);
  font-size: 12.5px;
  color: var(--ashell-text-muted);
  line-height: 1.6;
}

.thinking-content :deep(p) {
  margin: 0 0 6px;
}

.thinking-content :deep(p:last-child) {
  margin-bottom: 0;
}

.thinking-content :deep(pre) {
  margin: 4px 0;
  padding: 6px 8px;
  border-radius: 4px;
  background: var(--ashell-terminal-bg);
  border: 1px solid var(--ashell-border-soft);
  overflow-x: auto;
  font-size: 11px;
}

.msg.user .bubble-time {
  color: rgba(255, 255, 255, 0.6);
}

.bubble-avatar {
  flex-shrink: 0;
}

.assistant-avatar {
  background: linear-gradient(
    135deg,
    #7c5cff 0%,
    #4a8cff 100%
  ) !important;
  color: #fff !important;
}

.user-avatar {
  background: var(--ashell-panel-bg-soft) !important;
  color: var(--ashell-text-muted) !important;
}

.dot {
  color: var(--ashell-text-subtle);
  letter-spacing: 2px;
}

/* ── Typing indicator ── */

.bubble.typing {
  display: flex;
  gap: 4px;
  padding: 14px 12px;
}

.bubble.typing .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--ashell-text-subtle);
  animation: blink 1.2s infinite ease-in-out;
}

.bubble.typing .dot:nth-child(2) {
  animation-delay: 0.15s;
}

.bubble.typing .dot:nth-child(3) {
  animation-delay: 0.3s;
}

@keyframes blink {
  0%,
  80%,
  100% {
    opacity: 0.3;
    transform: scale(0.8);
  }
  40% {
    opacity: 1;
    transform: scale(1);
  }
}

/* ── Approval bar ── */

.approval-bar {
  display: flex;
  gap: 8px;
  padding: 8px 16px;
  border-top: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
}

/* ── Markdown inside assistant bubbles ── */

.markdown-body :deep(p) {
  margin: 0 0 6px;
}

.markdown-body :deep(p:last-child) {
  margin-bottom: 0;
}

.markdown-body :deep(pre) {
  margin: 6px 0;
  padding: 8px;
  border-radius: 4px;
  overflow-x: auto;
  background: var(--ashell-terminal-bg);
}

.markdown-body :deep(code) {
  font-family: var(--ashell-mono, "Fira Code", "JetBrains Mono", Menlo, Consolas, monospace);
  font-size: 12px;
}

.markdown-body :deep(:not(pre) > code) {
  padding: 1px 4px;
  border-radius: 3px;
  background: rgba(124, 92, 255, 0.15);
  font-size: 12px;
}

.markdown-body :deep(.code-lang) {
  font-size: 11px;
  color: var(--ashell-text-subtle);
  margin-bottom: 4px;
}

.markdown-body :deep(blockquote) {
  margin: 6px 0;
  padding: 4px 10px;
  border-left: 3px solid var(--ashell-primary);
  color: var(--ashell-text-muted);
}

.markdown-body :deep(ul) {
  margin: 4px 0;
  padding-left: 20px;
  list-style: disc outside;
}

.markdown-body :deep(ol) {
  margin: 4px 0;
  padding-left: 20px;
  list-style: decimal outside;
}

.markdown-body :deep(li) {
  margin: 2px 0;
}

.markdown-body :deep(strong) {
  color: var(--ashell-text-strong);
}

.markdown-body :deep(a) {
  color: var(--ashell-primary-hover);
  text-decoration: none;
}

.markdown-body :deep(a:hover) {
  text-decoration: underline;
}

.markdown-body :deep(details) {
  margin: 4px 0;
}

.markdown-body :deep(summary) {
  cursor: pointer;
  color: var(--ashell-text-subtle);
  font-size: 12px;
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--ashell-border-soft);
  margin: 8px 0;
}

.markdown-body :deep(table) {
  border-collapse: collapse;
  margin: 6px 0;
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid var(--ashell-border);
  padding: 4px 8px;
  font-size: 12px;
}

/* ── Composer ── */

.composer-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 16px 4px;
  font-size: 11px;
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.55));
  cursor: pointer;
  user-select: none;
  border-top: 1px solid var(--ashell-border-soft);
  flex-shrink: 0;
  transition: color 0.15s ease;
}

.composer-meta:hover {
  color: var(--ashell-text, rgba(255, 255, 255, 0.85));
}

.composer-meta.no-border {
  border-top: none;
}

.composer-meta-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.process-stop {
  flex-basis: 100%;
  display: flex;
  justify-content: center;
  padding-top: 4px;
}

.composer {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  padding: 4px 16px 16px;
  flex-shrink: 0;
}

.settings-hint {
  padding: 8px 12px;
  margin-top: 4px;
  font-size: 12px;
  color: var(--ashell-text-subtle, rgba(255, 255, 255, 0.55));
  background: var(--ashell-border-soft, rgba(255, 255, 255, 0.06));
  border-radius: 6px;
}
</style>
