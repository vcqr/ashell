import { describe, expect, spyOn, test } from "bun:test";
import {
  END_OF_RESPONSE,
  SESSION_STOP,
  emitAIMSG,
  emitAskUserQuestion,
  emitStopped,
  emitSystemError,
  emitThinking,
  emitToolCall,
  emitToolConfirm,
  emitToolRet,
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
    const lines = captureLines(() => emitAIMSG("hi"));
    expect(lines).toEqual(['[AIMSG]{"type":"assistant","payload":"hi"}']);
  });

  test("emitThinking 输出 [AI_THINKING]", () => {
    const lines = captureLines(() => emitThinking("hmm"));
    expect(lines).toEqual(['[AI_THINKING]{"type":"thinking","payload":"hmm"}']);
  });

  test("emitToolCall 省略 undefined 的 command/description（前端按缺省渲染）", () => {
    const lines = captureLines(() => emitToolCall({ name: "Read" }));
    expect(lines).toEqual(['[AITOOL]{"type":"assistant","payload":{"name":"Read"}}']);
  });

  test("emitToolCall 携带 command 与 description", () => {
    const lines = captureLines(() =>
      emitToolCall({ name: "Bash", command: "ls -la", description: "list files" }),
    );
    expect(lines).toEqual([
      '[AITOOL]{"type":"assistant","payload":{"name":"Bash","command":"ls -la","description":"list files"}}',
    ]);
  });

  test("emitToolRet 原样透传 payload 数组", () => {
    const payload = [{ type: "text", content: "result" }];
    const lines = captureLines(() => emitToolRet(payload));
    expect(lines).toEqual([
      '[TOOL_RET]{"type":"user","payload":[{"type":"text","content":"result"}]}',
    ]);
  });

  test("emitToolConfirm 携带 question 与 options", () => {
    const lines = captureLines(() =>
      emitToolConfirm("Allow this action? (y/n): ", { command: "rm -rf /" }),
    );
    expect(lines).toEqual([
      '[TOOL_CONFIRM]{"type":"tool_confirm","payload":{"question":"Allow this action? (y/n): ","options":{"command":"rm -rf /"}}}',
    ]);
  });

  test("emitAskUserQuestion 输出 title/items/tips", () => {
    const lines = captureLines(() =>
      emitAskUserQuestion({
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
    const lines = captureLines(() => emitSystemError("boom"));
    expect(lines).toEqual(['[SYSTEM_API_RETRY]{"type":"system","payload":"boom"}']);
  });

  test("emitStopped 输出 [STOPPED]", () => {
    const lines = captureLines(() => emitStopped());
    expect(lines[0]!.startsWith("[STOPPED]")).toBe(true);
    expect(JSON.parse(lines[0]!.slice("[STOPPED]".length)).type).toBe("system");
  });

  test("控制行常量与前端解析约定一致", () => {
    expect(END_OF_RESPONSE).toBe("[END_OF_RESPONSE]");
    expect(SESSION_STOP.startsWith("[SESSION_STOP]")).toBe(true);
  });
});
