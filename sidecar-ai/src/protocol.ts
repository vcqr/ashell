/**
 * 前端输出协议（stdout 行协议）。
 *
 * AiAssistant.vue 按 [TAG] 前缀解析 sidecar 的 stdout，每行一条消息；
 * JSON 信封统一为 { type, payload }。本模块是协议唯一出口，
 * 各引擎适配器不得自行拼接协议行。
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

function emit(tag: string, msg: AIMsg): void {
  console.log(`${tag}${JSON.stringify(msg)}`);
}

/** assistant 正文消息 */
export function emitAIMSG(payload: string): void {
  emit("[AIMSG]", { type: "assistant", payload });
}

/** thinking 内容 */
export function emitThinking(payload: string): void {
  emit("[AI_THINKING]", { type: "thinking", payload });
}

/** 工具调用展示（command 缺省时前端只渲染工具名） */
export function emitToolCall(tool: AITool): void {
  emit("[AITOOL]", { type: "assistant", payload: tool });
}

/** 工具返回内容，payload 为 [{ type, content }] 数组 */
export function emitToolRet(payload: unknown): void {
  emit("[TOOL_RET]", { type: "user", payload });
}

/** 请求用户审批（y/n），options 按 KV 渲染，key 为 command 时走代码块 */
export function emitToolConfirm(question: string, options?: unknown): void {
  emit("[TOOL_CONFIRM]", {
    type: "tool_confirm",
    payload: { question, options },
  });
}

/** AskUserQuestion 选择题 */
export function emitAskUserQuestion(payload: AskUserQuestionPayload): void {
  emit("[AI_ASKUSERQUESTION]", { type: "AskUserQuestion", payload });
}

/** 系统级错误 / API 重试提示 */
export function emitSystemError(payload: string): void {
  emit("[SYSTEM_API_RETRY]", { type: "system", payload });
}

/** 用户中断本轮响应（payload 目前不被前端使用） */
export function emitStopped(): void {
  emit("[STOPPED]", { type: "system", payload: "用户已中断本次响应" });
}
