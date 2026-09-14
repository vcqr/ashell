/**
 * 前端输出协议。
 *
 * 两种传输形态共用本模块的 emit 函数（协议唯一出口，引擎不得自行拼协议行）：
 * - 旧版单会话模式：直接 console.log 输出 `[TAG]{json}` 行；
 * - daemon 多路复用模式：经 setEmitSink 注册的 sink 把 (sid, tag, body/tail)
 *   包成 NDJSON 帧交给宿主（见 daemon.ts），由宿主按 ssid 解复用。
 *
 * JSON 信封统一为 { type, payload }。
 */

export interface AIMsg {
  type: string;
  payload?: unknown;
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
  if (emitSink) {
    emitSink(sid, tag, msg, "");
    return;
  }
  console.log(`${tag}${JSON.stringify(msg)}`);
}

/** 控制行输出（END_OF_RESPONSE / SESSION_STOP 等 tag-only 行） */
export function emitControl(sid: string, line: string): void {
  const m = /^(\[[A-Z_]+\])/.exec(line);
  if (emitSink && m) {
    emitSink(sid, m[1]!, null, line.slice(m[1]!.length));
    return;
  }
  console.log(line);
}

/** assistant 正文消息 */
export function emitAIMSG(sid: string, payload: string): void {
  emit(sid, "[AIMSG]", { type: "assistant", payload });
}

/** thinking 内容 */
export function emitThinking(sid: string, payload: string): void {
  emit(sid, "[AI_THINKING]", { type: "thinking", payload });
}

/** 工具调用展示（command 缺省时前端只渲染工具名） */
export function emitToolCall(sid: string, tool: AITool): void {
  emit(sid, "[AITOOL]", { type: "assistant", payload: tool });
}

/** 工具返回内容，payload 为 [{ type, content }] 数组 */
export function emitToolRet(sid: string, payload: unknown): void {
  emit(sid, "[TOOL_RET]", { type: "user", payload });
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

/** 用户中断本轮响应（payload 目前不被前端使用） */
export function emitStopped(sid: string): void {
  emit(sid, "[STOPPED]", { type: "system", payload: "用户已中断本次响应" });
}
