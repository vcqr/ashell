import { describe, expect, spyOn, test } from "bun:test";
import { askUserQuestion } from "./askUser";
import { StdinIO } from "./stdin";

const singleQ = {
  header: "Mode",
  question: "Pick one",
  options: [{ label: "Alpha" }, { label: "Beta" }, { label: "Gamma" }],
};

/** 启动提问（拦截 emit 日志）并在后台喂答案 */
function startAsk(io: StdinIO) {
  const spy = spyOn(console, "log").mockImplementation(() => {});
  const promise = askUserQuestion(io, singleQ);
  return { promise, spy };
}

describe("askUserQuestion", () => {
  test("数字回答映射为选项 label", async () => {
    const io = new StdinIO(false);
    const { promise, spy } = startAsk(io);
    expect(spy.mock.calls.length).toBeGreaterThan(0);
    const emitted = String(spy.mock.calls[0]![0]);
    spy.mockRestore();

    expect(emitted.startsWith("[AI_ASKUSERQUESTION]")).toBe(true);
    const envelope = JSON.parse(emitted.slice("[AI_ASKUSERQUESTION]".length));
    expect(envelope.payload.title).toBe("Mode: Pick one");
    expect(envelope.payload.items).toEqual(["1. Alpha", "2. Beta", "3. Gamma"]);
    expect(envelope.payload.tips).toContain("(Enter a number");

    io.handleLine("2");
    expect(await promise).toBe("Beta");
  });

  test("多选逗号分隔映射为 label 列表", async () => {
    const io = new StdinIO(false);
    const spy = spyOn(console, "log").mockImplementation(() => {});
    const promise = askUserQuestion(io, { ...singleQ, multiSelect: true });
    io.handleLine("1,3");
    spy.mockRestore();
    expect(await promise).toBe("Alpha, Gamma");
  });

  test("非法序号回退为原始输入透传", async () => {
    const io = new StdinIO(false);
    const spy = spyOn(console, "log").mockImplementation(() => {});
    const promise = askUserQuestion(io, singleQ);
    io.handleLine("99");
    spy.mockRestore();
    expect(await promise).toBe("99");
  });

  test("非数字自由文本原样返回", async () => {
    const io = new StdinIO(false);
    const spy = spyOn(console, "log").mockImplementation(() => {});
    const promise = askUserQuestion(io, singleQ);
    io.handleLine("do it yourself");
    spy.mockRestore();
    expect(await promise).toBe("do it yourself");
  });

  test("__STOP__ 打断时返回 null", async () => {
    const io = new StdinIO(false);
    const spy = spyOn(console, "log").mockImplementation(() => {});
    const promise = askUserQuestion(io, singleQ);
    io.handleLine("__STOP__");
    spy.mockRestore();
    expect(await promise).toBe(null);
  });

  test("带 description 的选项渲染进 items", async () => {
    const io = new StdinIO(false);
    const spy = spyOn(console, "log").mockImplementation(() => {});
    const promise = askUserQuestion(io, {
      header: "H",
      question: "Q",
      options: [{ label: "A", description: "the a" }],
    });
    const emitted = String(spy.mock.calls[0]![0]);
    spy.mockRestore();
    expect(JSON.parse(emitted.slice("[AI_ASKUSERQUESTION]".length)).payload.items).toEqual([
      "1. A - the a",
    ]);
    io.handleLine("1");
    await promise;
  });
});
