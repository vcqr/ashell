/**
 * 前端输出协议。
 *
 * 两种传输形态共用本模块的 emit 函数（协议唯一出口，引擎不得自行拼协议行）：
 * - 旧版单会话模式：直接 console.log 输出 `[TAG]{json}` 行；
 * - daemon 多路复用模式：经 setEmitSink 注册的 sink 把 (sid, tag, body/tail)
 *   包成 NDJSON 帧交给宿主（见 daemon.ts），由宿主按 ssid 解复用。
 *
 * JSON 信封统一为 { type, payload }。
 *
 * streaming 增量帧（AI_STREAMING=1 开启）构成显式的分段生命周期：
 *   [AI_TEXT_START]/[AI_THINKING_START] → [AIMSG_DELTA]/[AI_THINKING_DELTA]* → [AIMSG]/[AI_THINKING](全文)
 * 引擎在块结束时立即发全文帧（FINAL，权威文本）；任何其他帧发出前先 flush 该
 * 会话的待发增量，保证段内顺序。FINAL 在前端有两种语义：有活跃同类气泡时覆盖
 * 续写（打字机继续打完），无活跃气泡时按普通消息入列（block 模式只有 FINAL 帧，
 * 走后者的旧语义，两端兼容）。
 */

export interface AIMsg {
  type: string;
  payload?: unknown;
  /**
   * 工具关联 id（仅 [AITOOL]/[TOOL_RET] 帧可选携带）：把"调用行"与"结果行"
   * 配对，前端据此给尚未返回结果的调用行渲染"运行中"状态。
   * 可选字段：旧前端忽略未知字段，向后兼容。
   */
  toolCallId?: string;
  /** 工具失败标记（仅 [TOOL_RET] 帧可选携带）：前端渲染失败样式的结果行 */
  isError?: boolean;
}

export interface AITool {
  name: string;
  command?: string;
  description?: string;
}

export interface AskUserQuestionPayload {
  title: string;
  items?: string[];
  tips?: string;
  /** 多选题：可逗号分隔多选序号；前端渲染为可勾选+确认提交 */
  multiSelect?: boolean;
}

/** 单轮对话的 token 用量统计（[AI_USAGE] 帧负载） */
export interface AiUsageStats {
  /** 本轮输入 token（不含缓存命中/写入部分） */
  input: number;
  /** 本轮输出 token */
  output: number;
  /** 缓存读取 token */
  cacheRead: number;
  /** 缓存写入 token */
  cacheWrite: number;
  /** 本轮成本（美元），无价格信息时缺省 */
  cost?: number;
  /** 轮末上下文占用 token 估算 */
  contextTokens?: number;
  /** 模型上下文窗口大小 */
  contextWindow?: number;
  /** 上下文占用百分比（0-100+） */
  contextPercent?: number;
}

/** 上下文压缩事件（[AI_COMPACT] 帧负载） */
export interface AiCompactInfo {
  trigger: "manual" | "auto";
  /** 压缩前上下文 token */
  tokensBefore: number;
  /** 压缩后 token 估算 */
  tokensAfter?: number;
}

export const END_OF_RESPONSE = "[END_OF_RESPONSE]";
export const SESSION_STOP = "[SESSION_STOP]The conversation has ended.";

/**
 * daemon 模式的 emit sink：(sid, tag, body, tail)。
 * body 非 null 时宿主还原为 `[tag]` + JSON.stringify(body)；
 * body 为 null 时为纯控制行，还原为 `[tag]` + tail 文本。
 */
export type EmitSink = (sid: string, tag: string, body: unknown | null, tail: string) => void;

let emitSink: EmitSink | null = null;

/** 仅 daemon 启动时注册一次；单会话模式保持 null（直写 stdout） */
export function setEmitSink(sink: EmitSink | null): void {
  emitSink = sink;
}

function emit(sid: string, tag: string, msg: AIMsg): void {
  flushPendingDeltas(sid);
  if (emitSink) {
    emitSink(sid, tag, msg, "");
    return;
  }
  console.log(`${tag}${JSON.stringify(msg)}`);
}

/** 控制行输出（END_OF_RESPONSE / SESSION_STOP 等 tag-only 行） */
export function emitControl(sid: string, line: string): void {
  flushPendingDeltas(sid);
  const m = /^(\[[A-Z_]+\])/.exec(line);
  if (emitSink && m) {
    emitSink(sid, m[1]!, null, line.slice(m[1]!.length));
    return;
  }
  console.log(line);
}

/* ── streaming 增量帧（AI_STREAMING=1 开启，灰度开关两引擎通用） ── */

/** streaming 是否开启：仅在 ~/.ashell/ai/.env 显式 AI_STREAMING=1/true 时启用 */
export function isStreamingEnabled(): boolean {
  return process.env.AI_STREAMING === "1" || process.env.AI_STREAMING === "true";
}

/** 增量帧负载：seq 为每会话递增序号（仅诊断用，传输层本身有序无重排） */
export interface AiDeltaPayload {
  seq: number;
  text: string;
}

type DeltaKind = "text" | "thinking";

const DELTA_TAGS: Record<DeltaKind, string> = {
  text: "[AIMSG_DELTA]",
  thinking: "[AI_THINKING_DELTA]",
};
const DELTA_TYPES: Record<DeltaKind, string> = {
  text: "assistant_delta",
  thinking: "thinking_delta",
};

/** 聚合窗口：窗口内的片段合并成一帧发出，避免逐 token 打爆 WS/Tauri event */
const DELTA_FLUSH_MS = 50;

interface DeltaBuffer {
  sid: string;
  kind: DeltaKind;
  text: string;
  timer: ReturnType<typeof setTimeout> | null;
}

const deltaBuffers = new Map<string, DeltaBuffer>();
const deltaSeq = new Map<string, number>();

function deltaKey(sid: string, kind: DeltaKind): string {
  return `${sid}\u0000${kind}`;
}

/** 立即出帧缓冲中的增量；空缓冲无害 */
function flushDeltaBuffer(buf: DeltaBuffer): void {
  if (buf.timer !== null) {
    clearTimeout(buf.timer);
    buf.timer = null;
  }
  if (!buf.text) {
    deltaBuffers.delete(deltaKey(buf.sid, buf.kind));
    return;
  }
  const seq = (deltaSeq.get(buf.sid) ?? 0) + 1;
  deltaSeq.set(buf.sid, seq);
  const text = buf.text;
  buf.text = "";
  emit(buf.sid, DELTA_TAGS[buf.kind], {
    type: DELTA_TYPES[buf.kind],
    payload: { seq, text },
  });
}

/** 把某会话所有待发增量立即出帧：任何其他帧（含校准全帧、END_OF_RESPONSE）
 *  发出前调用，保证增量与全帧的相对顺序。
 *  重入保护：delta 帧本身也走 emit()，若不挡住会把其他 kind 的缓冲提前带出，
 *  破坏外层循环确定的帧顺序 */
let flushingDeltas = false;

function flushPendingDeltas(sid: string): void {
  if (flushingDeltas) return;
  flushingDeltas = true;
  try {
    for (const kind of ["text", "thinking"] as const) {
      const buf = deltaBuffers.get(deltaKey(sid, kind));
      if (buf) flushDeltaBuffer(buf);
    }
  } finally {
    flushingDeltas = false;
  }
}

/** 测试/调试辅助：手动 flush 指定会话的待发增量（正常由 emit 路径自动触发） */
export function flushSessionDeltas(sid: string): void {
  flushPendingDeltas(sid);
}

/** 引擎入口：累积增量片段，按窗口聚合出帧（streaming 关闭时为 no-op） */
function pushDelta(sid: string, kind: DeltaKind, text: string): void {
  if (!isStreamingEnabled() || !text) return;
  const key = deltaKey(sid, kind);
  let buf = deltaBuffers.get(key);
  if (!buf) {
    buf = { sid, kind, text: "", timer: null };
    deltaBuffers.set(key, buf);
  }
  buf.text += text;
  if (buf.timer === null) {
    buf.timer = setTimeout(() => {
      buf!.timer = null;
      flushDeltaBuffer(buf!);
    }, DELTA_FLUSH_MS);
  }
}

/** assistant 正文增量片段 */
export function emitAIMSGDelta(sid: string, text: string): void {
  pushDelta(sid, "text", text);
}

/** thinking 增量片段 */
export function emitThinkingDelta(sid: string, text: string): void {
  pushDelta(sid, "thinking", text);
}

/**
 * 内容块开始帧（仅 streaming 模式）：引擎每开始一个 text/thinking 块发一帧，
 * 前端据此关闭上一个流式气泡并新开一个。与 DELTA/FINAL 构成显式的分段生命周期：
 * START → DELTA* → FINAL(全文)，段与段之间严格串行，前端无需猜测帧归属。
 */
export function emitTextStart(sid: string): void {
  if (!isStreamingEnabled()) return;
  emit(sid, "[AI_TEXT_START]", { type: "assistant_text_start" });
}

export function emitThinkingStart(sid: string): void {
  if (!isStreamingEnabled()) return;
  emit(sid, "[AI_THINKING_START]", { type: "thinking_start" });
}

/** assistant 正文消息 */
export function emitAIMSG(sid: string, payload: string): void {
  emit(sid, "[AIMSG]", { type: "assistant", payload });
}

/** thinking 内容 */
export function emitThinking(sid: string, payload: string): void {
  emit(sid, "[AI_THINKING]", { type: "thinking", payload });
}

/** 工具调用展示（command 缺省时前端只渲染工具名）；toolCallId 供前端与结果行配对 */
export function emitToolCall(sid: string, tool: AITool, toolCallId?: string): void {
  emit(sid, "[AITOOL]", { type: "assistant", payload: tool, ...(toolCallId && { toolCallId }) });
}

/** 工具返回内容，payload 为 [{ type, content }] 数组；toolCallId 对应 [AITOOL] 帧，isError 标记失败结果 */
export function emitToolRet(sid: string, payload: unknown, toolCallId?: string, isError?: boolean): void {
  emit(sid, "[TOOL_RET]", {
    type: "user",
    payload,
    ...(toolCallId && { toolCallId }),
    ...(isError && { isError: true }),
  });
}

/** 请求用户审批（y/n），options 按 KV 渲染，key 为 command 时走代码块 */
export function emitToolConfirm(sid: string, question: string, options?: unknown): void {
  emit(sid, "[TOOL_CONFIRM]", {
    type: "tool_confirm",
    payload: { question, options },
  });
}

/** AskUserQuestion 选择题 */
export function emitAskUserQuestion(sid: string, payload: AskUserQuestionPayload): void {
  emit(sid, "[AI_ASKUSERQUESTION]", { type: "AskUserQuestion", payload });
}

/** 系统级错误 / API 重试提示 */
export function emitSystemError(sid: string, payload: string): void {
  emit(sid, "[SYSTEM_API_RETRY]", { type: "system", payload });
}

/** 每轮对话结束后的 token 用量统计 */
export function emitUsage(sid: string, payload: AiUsageStats): void {
  emit(sid, "[AI_USAGE]", { type: "usage", payload });
}

/** 上下文压缩事件 */
export function emitCompact(sid: string, payload: AiCompactInfo): void {
  emit(sid, "[AI_COMPACT]", { type: "compact", payload });
}

/** 用户中断本轮响应（payload 目前不被前端使用） */
export function emitStopped(sid: string): void {
  emit(sid, "[STOPPED]", { type: "system", payload: "用户已中断本次响应" });
}
