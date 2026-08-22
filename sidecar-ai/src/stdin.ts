/**
 * stdin 行读取 + 停止信号。
 *
 * 宿主通过 stdin 下发用户消息与控制命令：
 * - "__STOP__"  中断当前轮次（触发 onStop 回调并释放所有交互等待）
 * - "__QUIT__"  作为普通数据行返回，由主循环结束会话
 */

/** readLineOrStop 在停止信号到达时的返回哨兵值 */
export const STOP_SENTINEL = "__STOPPED__";

type LineResolver = (line: string) => void;

export class StdinIO {
  /** 停止信号到达时的回调（index.ts 接到 engine.stop 上） */
  onStop: (() => void) | null = null;

  private buffer: string[] = [];
  private waiters: LineResolver[] = [];
  /** 挂起中的可中断等待，stop 时统一释放 */
  private stoppable = new Set<LineResolver>();
  private lineBuffer = "";

  /**
   * @param wired 是否接管真实 process.stdin；单测传 false，
   *              用 handleLine/handleEnd 直接注入数据
   */
  constructor(wired = true) {
    if (!wired) return;
    process.stdin.setEncoding("utf-8");
    process.stdin.on("data", (chunk: string) => this.feedChunk(chunk));
    process.stdin.on("end", () => this.handleEnd());
  }

  /** 处理一个原始分片：按 \n 切行，整行交给 handleLine，残行留缓冲 */
  feedChunk(chunk: string): void {
    this.lineBuffer += chunk;
    const lines = this.lineBuffer.split("\n");
    this.lineBuffer = lines.pop() || "";
    for (const line of lines) this.handleLine(line);
  }

  /** 处理一条完整数据行（生产路径由 data 监听器分片后调用） */
  handleLine(line: string): void {
    const trimmed = line.trim();
    if (!trimmed) return;

    // 停止是控制信号：不作为数据行消费
    if (trimmed === "__STOP__") {
      console.log("[SIDECAR] Received stop command, aborting current query...");
      this.signalStop();
      this.onStop?.();
      return;
    }

    if (this.waiters.length > 0) {
      this.waiters.shift()!(trimmed);
    } else {
      this.buffer.push(trimmed);
    }
  }

  /** stdin 结束时冲刷残留的半行 */
  handleEnd(): void {
    console.log("[STDIN] End of input");
    if (!this.lineBuffer.trim()) return;
    const line = this.lineBuffer.trim();
    if (this.waiters.length > 0) {
      this.waiters.shift()!(line);
    } else {
      this.buffer.push(line);
    }
  }

  /** 读一行（不感知停止信号）—— 主循环用 */
  readLine(): Promise<string> {
    if (this.buffer.length > 0) {
      return Promise.resolve(this.buffer.shift()!);
    }
    return new Promise((resolve) => {
      this.waiters.push(resolve);
    });
  }

  /**
   * 读一行；停止信号到达时所有挂起的等待统一返回 STOP_SENTINEL。
   * 审批 / 提问等交互等待必须走这里，保证 __STOP__ 能打断。
   */
  readLineOrStop(): Promise<string> {
    if (this.buffer.length > 0) {
      return Promise.resolve(this.buffer.shift()!);
    }
    return new Promise((resolve) => {
      const entry: LineResolver = (line) => {
        this.stoppable.delete(entry);
        resolve(line);
      };
      this.stoppable.add(entry);
      this.waiters.push(entry);
    });
  }

  /** 释放所有挂起的 readLineOrStop（返回 STOP_SENTINEL）；不影响普通 readLine */
  signalStop(): void {
    const pending = [...this.stoppable];
    this.stoppable.clear();
    for (const entry of pending) {
      const idx = this.waiters.indexOf(entry);
      if (idx >= 0) this.waiters.splice(idx, 1);
      entry(STOP_SENTINEL);
    }
  }
}
