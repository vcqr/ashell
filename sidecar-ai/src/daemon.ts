/**
 * AI sidecar daemon（多路复用常驻服务）。
 *
 * 宿主（Rust）按需拉起 `app-ai --serve`，全应用仅一个实例；进程内按 ssid
 * 承载多个 AI 会话，stdin/stdout 升级为带会话号的 NDJSON 帧总线：
 *
 * 宿主 → daemon（stdin，每行一帧）：
 *   {"v":1,"type":"create","id":N,"sid":"..","engine":"claude","workspace":"..","token":"..","addr":".."}
 *   {"v":1,"type":"user","sid":"..","data":".."}      // 等价旧版 stdin 数据行
 *   {"v":1,"type":"stop","sid":".."}                  // 等价旧版 __STOP__
 *   {"v":1,"type":"close","sid":".."}                 // 结束并回收会话
 *
 * daemon → 宿主（stdout，每行一帧）：
 *   {"v":1,"type":"event","sid":"..","tag":"[AIMSG]","body":{type,payload}}
 *   {"v":1,"type":"event","sid":"..","tag":"[END_OF_RESPONSE]","tail":""}
 *   {"v":1,"id":N,"type":"ok"} / {"v":1,"id":N,"type":"err","error":".."}
 *
 * 会话隔离约定：
 * - 每会话一个 StdinIO(wired=false) 收件箱 + 独立 EngineAdapter，帧按 sid 路由；
 * - .env 在每次 create 时重读（模型/供应商切换经宿主物化到 .env，新会话即生效）；
 * - 进程级 env 为全局共享：claude CLI 子进程每轮继承的是「最近一次 create
 *   之后的 env」。配置只有一份全局 .env，跨会话不存在第二套配置，因此不会
 *   串内容，只存在「切换后旧会话下一轮用新配置」这一与单进程版不同的语义。
 */
import { config as loadDotenv } from "dotenv";
import { existsSync } from "fs";
import { resolve } from "path";
import { createEngine } from "./engines/factory";
import type { EngineAdapter, EngineContext } from "./engines/types";
import {
  END_OF_RESPONSE,
  emitControl,
  emitStopped,
  emitSystemError,
  setEmitSink,
  type EmitSink,
} from "./protocol";
import { StdinIO } from "./stdin";

/** create 时需要清掉再由 .env 重载的受管 env key（.env 之外来源一律不保留） */
const MANAGED_ENV_KEYS = [
  "ANTHROPIC_API_KEY",
  "ANTHROPIC_BASE_URL",
  "ANTHROPIC_MODEL",
  "ANTHROPIC_AUTH_TOKEN",
  "CLAUDE_CLI_PATH",
  "SIDECAR_TYPE",
  "PI_PROVIDER",
  "PI_MODEL",
  "PI_BASE_URL",
  "PI_API_KEY",
  "PI_API",
  "PI_THINKING_LEVEL",
];

interface CreateFrame {
  v: 1;
  type: "create";
  id?: number;
  sid: string;
  engine?: string;
  workspace: string;
  token?: string;
  addr?: string;
}

type InFrame =
  | CreateFrame
  | { v: 1; type: "user"; sid: string; data?: string }
  | { v: 1; type: "stop"; sid: string }
  | { v: 1; type: "close"; sid: string };

/** 单个会话的运行时状态（对应旧版的一个 sidecar 进程） */
interface Session {
  sid: string;
  homedir: string;
  /** 本会话私有收件箱：等价旧版 StdinIO，只是数据来自帧路由而非 process.stdin */
  io: StdinIO;
  adapter: EngineAdapter | null;
  /** create 完成（引擎就绪、runner 已可运行） */
  ready: boolean;
  closing: boolean;
  runner: Promise<void> | null;
}

/** 主循环用 QUIT 哨兵：teardown 投递一行让挂起的 readLine 返回 */
const QUIT_LINE = "__QUIT__";

export interface DaemonOptions {
  /** 帧写回（默认 process.stdout），测试注入 */
  write?: (line: string) => void;
  /** 引擎工厂（默认真实 factory），测试注入假引擎 */
  createEngineFn?: typeof createEngine;
}

export class DaemonCore {
  private sessions = new Map<string, Session>();
  private write: (line: string) => void;
  private createEngineFn: typeof createEngine;

  constructor(opts: DaemonOptions = {}) {
    this.write = opts.write ?? ((line: string) => process.stdout.write(line));
    this.createEngineFn = opts.createEngineFn ?? createEngine;
    setEmitSink(this.sink);
  }

  /** 协议事件按会话出帧：body 带 JSON 信封；纯控制行走 tail */
  private sink: EmitSink = (sid, tag, body, tail) => {
    const frame =
      body !== null
        ? { v: 1, type: "event", sid, tag, body }
        : { v: 1, type: "event", sid, tag, tail };
    this.emitFrame(frame);
  };

  /** NDJSON 帧统一出口：必须带结尾换行，宿主按行分帧 */
  private emitFrame(frame: Record<string, unknown>): void {
    this.write(JSON.stringify(frame) + "\n");
  }

  private reply(id: number | undefined, ok: boolean, error?: string): void {
    if (id === undefined) return;
    const frame = ok
      ? { v: 1, id, type: "ok" }
      : { v: 1, id, type: "err", error: error ?? "" };
    this.emitFrame(frame);
  }

  /** 处理一条 stdin 帧行；不合帧的行忽略（宿主侧同样只放行合法帧） */
  handleLine(line: string): void {
    const trimmed = line.trim();
    if (!trimmed) return;

    let frame: InFrame;
    try {
      frame = JSON.parse(trimmed) as InFrame;
    } catch {
      console.warn("[DAEMON] non-frame stdin line dropped");
      return;
    }
    if (
      !frame ||
      (frame as { v?: unknown }).v !== 1 ||
      typeof (frame as { type?: unknown }).type !== "string"
    ) {
      console.warn("[DAEMON] invalid frame dropped:", trimmed.slice(0, 120));
      return;
    }

    switch (frame.type) {
      case "create":
        // async 派发兜底：bun 对未处理 rejection 直接崩进程，必须就地接住
        this.handleCreate(frame).catch((error) => {
          console.error("[DAEMON] create dispatch failed:", error);
          this.reply(frame.id, false, error instanceof Error ? error.message : String(error));
        });
        return;
      case "user": {
        const s = this.sessions.get(frame.sid);
        // 数据行一律投递：会话未 ready 时先缓冲，runner 启动后消费
        s?.io.handleLine(frame.data ?? "");
        return;
      }
      case "stop": {
        this.sessions.get(frame.sid)?.io.handleLine("__STOP__");
        return;
      }
      case "close":
        this.requestClose(frame.sid);
        return;
      default:
        console.warn("[DAEMON] unknown frame type:", String((frame as { type?: unknown }).type));
    }
  }

  private async handleCreate(frame: CreateFrame): Promise<void> {
    const sid = frame.sid;
    if (!sid || !frame.workspace) {
      this.reply(frame.id, false, "create frame requires sid and workspace");
      return;
    }
    // Windows 反斜杠统一成正斜杠（与旧版入口一致）
    const homedir = String(frame.workspace).replace(/\\/g, "/");
    if (!existsSync(homedir)) {
      this.reply(frame.id, false, `workspace not found: ${homedir}`);
      return;
    }

    // 同 ssid 重复 create（用户「新对话」）：先请求旧会话终止
    const old = this.sessions.get(sid);
    if (old) this.teardown(old);

    applySessionEnv(homedir);

    const io = new StdinIO(false);
    const session: Session = {
      sid,
      homedir,
      io,
      adapter: null,
      ready: false,
      closing: false,
      runner: null,
    };
    this.sessions.set(sid, session);

    try {
      const ctx: EngineContext = {
        homedir,
        ssid: sid,
        token: String(frame.token ?? ""),
        addr: String(frame.addr ?? ""),
        io,
      };
      session.adapter = await this.createEngineFn(frame.engine?.trim() || "claude", ctx);
    } catch (error) {
      this.sessions.delete(sid);
      const msg = error instanceof Error ? error.message : String(error);
      console.error("[DAEMON] create engine failed:", msg);
      this.reply(frame.id, false, msg);
      return;
    }

    io.onStop = () => session.adapter?.stop();

    if (session.closing) {
      // create 期间收到 close：创建结果有效但不启动 runner，直接回收
      this.reply(frame.id, true);
      this.disposeSession(session);
      return;
    }

    session.ready = true;
    session.runner = this.runSession(session);
    this.reply(frame.id, true);
  }

  /** 单会话主循环：等价旧版 index.ts 的 while(true)，只是 per-session 化 */
  private async runSession(session: Session): Promise<void> {
    const { sid } = session;
    try {
      while (!session.closing) {
        const raw = await session.io.readLine();
        const trimmed = raw.trim();
        if (session.closing || !trimmed || trimmed === QUIT_LINE) break;
        const adapter = session.adapter;
        if (!adapter) break;

        try {
          await adapter.handle(trimmed);
        } catch (error) {
          if (!adapter.stopped) {
            console.error(`[DAEMON sid=${sid}] engine error:`, error);
            emitSystemError(sid, error instanceof Error ? error.message : String(error));
          }
        }
        if (adapter.stopped) emitStopped(sid);
        emitControl(sid, END_OF_RESPONSE);
      }
    } catch (error) {
      console.error(`[DAEMON sid=${sid}] runner crashed:`, error);
    } finally {
      this.disposeSession(session);
    }
  }

  private disposeSession(session: Session): void {
    session.closing = true;
    // 注册表仍指向本会话时才删除：防止旧 runner 误删同 ssid 重建的新会话
    if (this.sessions.get(session.sid) === session) {
      this.sessions.delete(session.sid);
    }
    try {
      session.adapter?.dispose?.();
    } catch (error) {
      console.error(`[DAEMON sid=${session.sid}] dispose failed:`, error);
    }
  }

  /** 请求会话终止：打断当前轮 + 喂 QUIT 行解除主循环挂起 */
  private teardown(session: Session): void {
    session.closing = true;
    session.ready = false;
    session.io.signalStop();
    session.adapter?.stop();
    session.io.handleLine(QUIT_LINE);
  }

  requestClose(sid: string): void {
    const s = this.sessions.get(sid);
    if (s) this.teardown(s);
  }

  /** 测试辅助：当前会话数 */
  sessionCount(): number {
    return this.sessions.size;
  }
}

/**
 * create 时按会话工作目录重读 .env：先清掉受管 key 再 dotenv override 加载，
 * 保证「切换供应商/模型 → 宿主物化 .env → 新会话生效」的既有语义。
 */
function applySessionEnv(homedir: string): void {
  for (const key of MANAGED_ENV_KEYS) delete process.env[key];
  loadDotenv({ path: resolve(homedir, ".env"), override: true, quiet: true });
}

/** 启动 daemon：接管 process.stdin 帧流；stdout 只出协议帧 */
export async function runDaemon(opts: DaemonOptions = {}): Promise<void> {
  // stdout 承载帧协议：把所有调试日志重定向到 stderr，防止杂散输出破坏分帧
  console.log = (...args: unknown[]) => console.warn(...args);

  const core = new DaemonCore(opts);

  let lineBuffer = "";
  process.stdin.setEncoding("utf-8");
  process.stdin.on("data", (chunk: string) => {
    lineBuffer += chunk;
    const lines = lineBuffer.split("\n");
    lineBuffer = lines.pop() || "";
    for (const line of lines) core.handleLine(line);
  });

  // 挂起直至宿主关闭 stdin（EOF）：daemon 生命周期跟随宿主进程，
  // 由 index.ts 的 main().finally 在本函数返回后统一退出
  const eof = new Promise<void>((resolve) => process.stdin.once("end", resolve));

  // 预热引擎模块：首个 create 免去 SDK 动态加载（实测 ~1.5s），让"打开面板
  // 预建会话"近乎瞬时。预热失败不致命——create 时会重新加载并如实报错。
  void Promise.all([import("./engines/claude"), import("./engines/pi")]).catch(
    (error) => console.warn("[DAEMON] engine preload failed:", error),
  );

  // 孤儿自愈兜底：宿主异常死亡（不走 kill_all）时进程会被 init 收养
  // （ppid=1）。EOF 理论上会触发，但任何 fd 泄漏路径都可能让它失效，
  // 定期检查 ppid 确保孤儿必死。
  const watchdog = setInterval(() => {
    if (process.ppid === 1) {
      console.warn("[DAEMON] host process died (ppid=1), exiting");
      process.exit(0);
    }
  }, 10_000);

  await eof;
  clearInterval(watchdog);
  if (lineBuffer.trim()) core.handleLine(lineBuffer);
  console.warn("[DAEMON] stdin closed, exiting");
}
