import { describe, expect, test } from "bun:test";
import { STOP_SENTINEL, StdinIO } from "./stdin";

describe("StdinIO", () => {
  test("先到先缓冲，readLine 按 FIFO 消费；空行与首尾空白被规整", async () => {
    const io = new StdinIO(false);
    io.handleLine("a");
    io.handleLine("  b  ");
    io.handleLine("   ");
    io.handleLine("c");
    expect(await io.readLine()).toBe("a");
    expect(await io.readLine()).toBe("b");
    expect(await io.readLine()).toBe("c");
  });

  test("readLine 在缓冲为空时挂起，等后续行到达", async () => {
    const io = new StdinIO(false);
    const pending = io.readLine();
    io.handleLine("hello");
    expect(await pending).toBe("hello");
  });

  test("__STOP__ 触发 onStop、释放 readLineOrStop 为哨兵值，且不作为数据行消费", async () => {
    const io = new StdinIO(false);
    let stopped = false;
    io.onStop = () => {
      stopped = true;
    };
    const interactive = io.readLineOrStop();
    io.handleLine("__STOP__");
    expect(stopped).toBe(true);
    expect(await interactive).toBe(STOP_SENTINEL);

    // __STOP__ 不进缓冲：下一条读到的应是真正的数据行
    io.handleLine("real");
    expect(await io.readLineOrStop()).toBe("real");
  });

  test("stop 只释放交互等待；普通 readLine 不受影响继续等数据", async () => {
    const io = new StdinIO(false);
    let stoppedCount = 0;
    io.onStop = () => stoppedCount++;

    const i1 = io.readLineOrStop();
    const i2 = io.readLineOrStop();
    const plain = io.readLine();

    io.handleLine("__STOP__");
    expect(await i1).toBe(STOP_SENTINEL);
    expect(await i2).toBe(STOP_SENTINEL);
    expect(stoppedCount).toBe(1);

    // 普通 readLine 未被打断：喂入真实数据后才结算
    io.handleLine("next");
    expect(await plain).toBe("next");
  });

  test("分片输入按 \\n 切行，残行由 handleEnd 冲刷", async () => {
    const io = new StdinIO(false);
    const first = io.readLine();
    io.feedChunk("hel");
    io.feedChunk("lo\nwor");
    expect(await first).toBe("hello");

    io.handleEnd(); // stdin 结束冲刷半行
    expect(await io.readLine()).toBe("wor");
  });

  test("缓冲中已有行时，readLineOrStop 直接消费缓冲而非等待停止", async () => {
    const io = new StdinIO(false);
    io.handleLine("queued");
    expect(await io.readLineOrStop()).toBe("queued");
  });
});
