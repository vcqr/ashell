import { describe, expect, spyOn, test } from "bun:test";
import {
  END_OF_RESPONSE,
  SESSION_STOP,
  emitAIMSG,
  emitAIMSGDelta,
  emitAskUserQuestion,
  emitControl,
  emitStopped,
  emitSystemError,
  emitThinking,
  emitThinkingDelta,
  emitToolCall,
  emitToolConfirm,
  emitToolRet,
  flushSessionDeltas,
  emitTextStart,
  emitThinkingStart,
  isStreamingEnabled,
  setEmitSink,
  type EmitSink,
} from "./protocol";

/** 拦截 console.log，返回被吞掉的每一行。
 *  注意 bun:test 的 mockRestore() 会清空 mock.calls，必须在 restore 前读取 */
function captureLines(fn: () => void): string[] {
  const spy = spyOn(console, "log").mockImplementation(() => {});
  let lines: string[] = [];
  try {
    fn();
    lines = spy.mock.calls.map((call) => call.map((arg) => String(arg)).join(" "));
  } finally {
    spy.mockRestore();
  }
  return lines;
}

describe("protocol emitters", () => {
  test("emitAIMSG 输出 [AIMSG] + assistant 信封", () => {
    const lines = captureLines(() => emitAIMSG("s1", "hi"));
    expect(lines).toEqual(['[AIMSG]{"type":"assistant","payload":"hi"}']);
  });

  test("emitThinking 输出 [AI_THINKING]", () => {
    const lines = captureLines(() => emitThinking("s1", "hmm"));
    expect(lines).toEqual(['[AI_THINKING]{"type":"thinking","payload":"hmm"}']);
  });

  test("emitToolCall 省略 undefined 的 command/description（前端按缺省渲染）", () => {
    const lines = captureLines(() => emitToolCall("s1", { name: "Read" }));
    expect(lines).toEqual(['[AITOOL]{"type":"assistant","payload":{"name":"Read"}}']);
  });

  test("emitToolCall 携带 command 与 description", () => {
    const lines = captureLines(() =>
      emitToolCall("s1", { name: "Bash", command: "ls -la", description: "list files" }),
    );
    expect(lines).toEqual([
      '[AITOOL]{"type":"assistant","payload":{"name":"Bash","command":"ls -la","description":"list files"}}',
    ]);
  });

  test("emitToolRet 原样透传 payload 数组", () => {
    const payload = [{ type: "text", content: "result" }];
    const lines = captureLines(() => emitToolRet("s1", payload));
    expect(lines).toEqual([
      '[TOOL_RET]{"type":"user","payload":[{"type":"text","content":"result"}]}',
    ]);
  });

  test("emitToolConfirm 携带 question 与 options", () => {
    const lines = captureLines(() =>
      emitToolConfirm("s1", "Allow this action? (y/n): ", { command: "rm -rf /" }),
    );
    expect(lines).toEqual([
      '[TOOL_CONFIRM]{"type":"tool_confirm","payload":{"question":"Allow this action? (y/n): ","options":{"command":"rm -rf /"}}}',
    ]);
  });

  test("emitAskUserQuestion 输出 title/items/tips", () => {
    const lines = captureLines(() =>
      emitAskUserQuestion("s1", {
        title: "Mode: pick one",
        items: ["1. Alpha - first", "2. Beta"],
        tips: "(Enter a number, or type your own answer)",
      }),
    );
    expect(lines.length).toBe(1);
    expect(lines[0]!.startsWith("[AI_ASKUSERQUESTION]")).toBe(true);
    const envelope = JSON.parse(lines[0]!.slice("[AI_ASKUSERQUESTION]".length));
    expect(envelope.type).toBe("AskUserQuestion");
    expect(envelope.payload.title).toBe("Mode: pick one");
    expect(envelope.payload.items).toEqual(["1. Alpha - first", "2. Beta"]);
    expect(envelope.payload.tips).toContain("Enter a number");
  });

  test("emitSystemError 输出 [SYSTEM_API_RETRY]", () => {
    const lines = captureLines(() => emitSystemError("s1", "boom"));
    expect(lines).toEqual(['[SYSTEM_API_RETRY]{"type":"system","payload":"boom"}']);
  });

  test("emitStopped 输出 [STOPPED]", () => {
    const lines = captureLines(() => emitStopped("s1"));
    expect(lines[0]!.startsWith("[STOPPED]")).toBe(true);
    expect(JSON.parse(lines[0]!.slice("[STOPPED]".length)).type).toBe("system");
  });

  test("控制行常量与前端解析约定一致", () => {
    expect(END_OF_RESPONSE).toBe("[END_OF_RESPONSE]");
    expect(SESSION_STOP.startsWith("[SESSION_STOP]")).toBe(true);
  });
});

describe("protocol emit sink（daemon 模式）", () => {
  test("sink 接收 (sid, tag, body)，恢复 sink 后回到 stdout 直写", () => {
    const frames: Array<{ sid: string; tag: string; body: unknown | null; tail: string }> = [];
    const sink: EmitSink = (sid, tag, body, tail) => frames.push({ sid, tag, body, tail });
    setEmitSink(sink);
    try {
      emitAIMSG("A", "hello");
      emitControl("A", END_OF_RESPONSE);
      emitControl("A", SESSION_STOP);
    } finally {
      setEmitSink(null);
    }

    expect(frames).toHaveLength(3);
    expect(frames[0]).toEqual({
      sid: "A",
      tag: "[AIMSG]",
      body: { type: "assistant", payload: "hello" },
      tail: "",
    });
    expect(frames[1]).toEqual({ sid: "A", tag: "[END_OF_RESPONSE]", body: null, tail: "" });
    expect(frames[2]).toEqual({
      sid: "A",
      tag: "[SESSION_STOP]",
      body: null,
      tail: "The conversation has ended.",
    });

    // sink 撤销后回到旧版行为
    const lines = captureLines(() => emitAIMSG("B", "back"));
    expect(lines).toEqual(['[AIMSG]{"type":"assistant","payload":"back"}']);
  });
});

describe("protocol streaming 增量帧", () => {
  /** 收集 sink 帧，测试结束撤销 sink 并还原 AI_STREAMING */
  function withSink(fn: () => Promise<void> | void): Promise<Array<{ sid: string; tag: string; body: unknown | null; tail: string }>> {
    return (async () => {
      const frames: Array<{ sid: string; tag: string; body: unknown | null; tail: string }> = [];
      const sink: EmitSink = (sid, tag, body, tail) => frames.push({ sid, tag, body, tail });
      setEmitSink(sink);
      try {
        await fn();
      } finally {
        setEmitSink(null);
        delete process.env.AI_STREAMING;
      }
      return frames;
    })();
  }

  test("isStreamingEnabled 仅在 AI_STREAMING=1/true 时开启", () => {
    delete process.env.AI_STREAMING;
    expect(isStreamingEnabled()).toBe(false);
    process.env.AI_STREAMING = "0";
    expect(isStreamingEnabled()).toBe(false);
    process.env.AI_STREAMING = "1";
    expect(isStreamingEnabled()).toBe(true);
    process.env.AI_STREAMING = "true";
    expect(isStreamingEnabled()).toBe(true);
    delete process.env.AI_STREAMING;
  });

  test("streaming 关闭时 delta 为 no-op", async () => {
    delete process.env.AI_STREAMING;
    const frames = await withSink(() => {
      emitAIMSGDelta("off-1", "hello");
      flushSessionDeltas("off-1");
    });
    expect(frames).toEqual([]);
  });

  test("聚合窗口内多次 delta 合并为一帧，seq 递增，携带累积文本", async () => {
    process.env.AI_STREAMING = "1";
    const frames = await withSink(async () => {
      emitAIMSGDelta("s-delta", "he");
      emitAIMSGDelta("s-delta", "llo");
      emitThinkingDelta("s-delta", "hmm");
      // 窗口未到期：flush 手动触发，两个 kind 各出一帧
      flushSessionDeltas("s-delta");
      await new Promise((r) => setTimeout(r, 5));
    });
    expect(frames).toHaveLength(2);
    expect(frames[0]).toEqual({
      sid: "s-delta",
      tag: "[AIMSG_DELTA]",
      body: { type: "assistant_delta", payload: { seq: 1, text: "hello" } },
      tail: "",
    });
    expect(frames[1]).toEqual({
      sid: "s-delta",
      tag: "[AI_THINKING_DELTA]",
      body: { type: "thinking_delta", payload: { seq: 2, text: "hmm" } },
      tail: "",
    });
  });

  test("窗口到期自动 flush（~50ms）", async () => {
    process.env.AI_STREAMING = "1";
    const frames = await withSink(async () => {
      emitAIMSGDelta("s-timer", "tick");
      await new Promise((r) => setTimeout(r, 80));
    });
    expect(frames).toHaveLength(1);
    expect(frames[0]!.tag).toBe("[AIMSG_DELTA]");
    expect((frames[0]!.body as { payload: { text: string } }).payload.text).toBe("tick");
  });

  test("全帧发出前先 flush 待发增量（增量 → 校准全帧顺序）", async () => {
    process.env.AI_STREAMING = "1";
    const frames = await withSink(() => {
      emitAIMSGDelta("s-order", "part");
      emitAIMSG("s-order", "part whole");
    });
    expect(frames).toHaveLength(2);
    expect(frames[0]!.tag).toBe("[AIMSG_DELTA]");
    expect(frames[0]!.body).toEqual({ type: "assistant_delta", payload: { seq: 1, text: "part" } });
    expect(frames[1]!.tag).toBe("[AIMSG]");
    expect(frames[1]!.body).toEqual({ type: "assistant", payload: "part whole" });
  });

  test("END_OF_RESPONSE 前 flush 增量，轮末不丢尾帧", async () => {
    process.env.AI_STREAMING = "1";
    const frames = await withSink(() => {
      emitAIMSGDelta("s-end", "tail");
      emitControl("s-end", END_OF_RESPONSE);
    });
    expect(frames).toHaveLength(2);
    expect(frames[0]!.tag).toBe("[AIMSG_DELTA]");
    expect(frames[1]!.tag).toBe("[END_OF_RESPONSE]");
  });

  test("START 帧仅 streaming 模式发出，且先 flush 待发增量", async () => {
    process.env.AI_STREAMING = "0";
    const off = await withSink(() => {
      emitTextStart("s-start0");
      emitThinkingStart("s-start0");
    });
    expect(off).toEqual([]);

    process.env.AI_STREAMING = "1";
    const frames = await withSink(() => {
      emitAIMSGDelta("s-start", "seg1");
      emitTextStart("s-start");
    });
    expect(frames).toHaveLength(2);
    expect(frames[0]!.tag).toBe("[AIMSG_DELTA]");
    expect(frames[1]!.tag).toBe("[AI_TEXT_START]");
    expect(frames[1]!.body).toEqual({ type: "assistant_text_start" });
  });

  test("分段生命周期：START → DELTA → FINAL 全文帧顺序完整", async () => {
    process.env.AI_STREAMING = "1";
    const frames = await withSink(() => {
      emitThinkingStart("s-life");
      emitThinkingDelta("s-life", "think");
      emitThinking("s-life", "think hard");
      emitTextStart("s-life");
      emitAIMSGDelta("s-life", "hello ");
      emitAIMSGDelta("s-life", "world");
      emitAIMSG("s-life", "hello world");
    });
    expect(frames.map((f) => f.tag)).toEqual([
      "[AI_THINKING_START]",
      "[AI_THINKING_DELTA]",
      "[AI_THINKING]",
      "[AI_TEXT_START]",
      "[AIMSG_DELTA]",
      "[AIMSG]",
    ]);
  });
});
