import type { StdinIO } from "../stdin";

/**
 * 引擎运行上下文：启动参数 + 共享 stdin IO。
 * homedir 是 AI 工作目录（~/.ashell/ai），.env 与 CLAUDE_CLI_PATH 都以它为基准。
 */
export interface EngineContext {
  homedir: string;
  ssid: string;
  token: string;
  addr: string;
  io: StdinIO;
}

/**
 * 引擎适配器：把不同 Agent SDK 的驱动方式收敛到统一接口。
 * 一条用户消息 = 一次 handle() 调用；期间所有前端输出经 protocol 发出，
 * handle 返回后由主循环补发 [END_OF_RESPONSE] 与 [STOPPED]。
 */
export interface EngineAdapter {
  readonly name: string;
  /** 处理一条用户消息（一轮完整对话）。出错时 throw，由主循环统一上报。 */
  handle(userPrompt: string): Promise<void>;
  /** 中断当前轮次（__STOP__ 触发）；无进行中的轮次时为无害操作 */
  stop(): void;
  /** 当前轮是否被用户中断（handle 开始时复位；主循环据此发 [STOPPED]） */
  readonly stopped: boolean;
  /** 进程退出前清理 */
  dispose?(): void;
}
