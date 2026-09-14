import { describe, expect, test, afterAll } from "bun:test";
import { mkdtempSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { DaemonCore, type DaemonOptions } from "./daemon";
import type { EngineAdapter, EngineContext } from "./engines/types";
import { emitAIMSG, setEmitSink } from "./protocol";

/** 可控假引擎：记录 create/stop/dispose，handle 经受控 promise 阻塞 */
interface FakeEngine {
  adapter: EngineAdapter;
  record: { sid: string; ctx: EngineContext; stopCalls: number; disposeCalls: number };
  /** 置为 resolve 以放行当前 handle */
  release: () => void;
}

function makeFactory(created: FakeEngine[]) {
  return async (_type: string, ctx: EngineContext): Promise<EngineAdapter> => {
    const record = { sid: ctx.ssid, ctx, stopCalls: 0, disposeCalls: 0 };
    let resolver: (() => void) | null = null;
    const gate = new Promise<void>((r) => (resolver = r));
    const adapter: EngineAdapter = {
      name: "fake",
      get stopped() {
        return record.stopCalls > 0;
      },
      stop() {
        record.stopCalls += 1;
        resolver?.();
      },
      async handle(prompt: string) {
        await gate;
        // 被中断（stop/close）后与真实引擎一致：不再产出内容
        if (record.stopCalls > 0) return;
        emitAIMSG(ctx.ssid, `echo:${prompt}`);
      },
      dispose() {
        record.disposeCalls += 1;
      },
    };
    const fake: FakeEngine = { adapter, record, release: () => resolver?.() };
    created.push(fake);
    return adapter;
  };
}

function makeCore(created: FakeEngine[]) {
  const out: string[] = [];
  const opts: DaemonOptions = { write: (l) => out.push(l), createEngineFn: makeFactory(created) };
  return { core: new DaemonCore(opts), out };
}

function frames(out: string[]): any[] {
  return out.map((l) => JSON.parse(l));
}

/** 等 create 的 async 流程落地（reply 帧出现） */
async function settle(): Promise<void> {
  for (let i = 0; i < 50; i++) {
    await new Promise((r) => setTimeout(r, 1));
  }
}

function newWorkspace(): string {
  return mkdtempSync(join(tmpdir(), "ashell-daemon-test-"));
}

const SID = "sess-A";

afterAll(() => {
  // 撤销 DaemonCore 注册的全局 sink，避免污染其他测试文件
  setEmitSink(null);
});

describe("DaemonCore 帧路由", () => {
  test("create 成功 → ok 应答（带 id）", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    core.handleLine(
      JSON.stringify({
        v: 1,
        type: "create",
        id: 7,
        sid: SID,
        engine: "claude",
        workspace: newWorkspace(),
        token: "t",
        addr: "a",
      }),
    );
    await settle();

    const fr = frames(out);
    expect(fr).toEqual([{ v: 1, id: 7, type: "ok" }]);
    expect(core.sessionCount()).toBe(1);
  });

  test("user 消息 → 事件帧带正确 sid + END_OF_RESPONSE 收尾", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    core.handleLine(
      JSON.stringify({ v: 1, type: "create", id: 1, sid: SID, workspace: newWorkspace() }),
    );
    await settle();
    core.handleLine(JSON.stringify({ v: 1, type: "user", sid: SID, data: "hello" }));
    await created[0]!.release();
    await settle();

    const fr = frames(out).filter((f) => f.type === "event");
    expect(fr).toEqual([
      {
        v: 1,
        type: "event",
        sid: SID,
        tag: "[AIMSG]",
        body: { type: "assistant", payload: "echo:hello" },
      },
      { v: 1, type: "event", sid: SID, tag: "[END_OF_RESPONSE]", tail: "" },
    ]);
  });

  test("两个会话并发：输出按 sid 隔离，互不串流", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    const ws = newWorkspace();
    core.handleLine(JSON.stringify({ v: 1, type: "create", id: 1, sid: "A", workspace: ws }));
    core.handleLine(JSON.stringify({ v: 1, type: "create", id: 2, sid: "B", workspace: ws }));
    await settle();

    core.handleLine(JSON.stringify({ v: 1, type: "user", sid: "A", data: "from-A" }));
    core.handleLine(JSON.stringify({ v: 1, type: "user", sid: "B", data: "from-B" }));
    created[0]!.release();
    created[1]!.release();
    await settle();

    const bySid = (sid: string) =>
      frames(out)
        .filter((f) => f.type === "event" && f.sid === sid)
        .map((f) => f.body?.payload ?? f.tag);

    expect(bySid("A")).toEqual(["echo:from-A", "[END_OF_RESPONSE]"]);
    expect(bySid("B")).toEqual(["echo:from-B", "[END_OF_RESPONSE]"]);
  });

  test("stop 帧 → 引擎 stop 被调用，阻塞中的 handle 放行后补 STOPPED + END", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    core.handleLine(
      JSON.stringify({ v: 1, type: "create", id: 1, sid: SID, workspace: newWorkspace() }),
    );
    await settle();
    core.handleLine(JSON.stringify({ v: 1, type: "user", sid: SID, data: "long turn" }));

    core.handleLine(JSON.stringify({ v: 1, type: "stop", sid: SID }));
    await settle();
    expect(created[0]!.record.stopCalls).toBe(1);

    await created[0]!.release();
    await settle();
    const tags = frames(out)
      .filter((f) => f.type === "event" && f.sid === SID)
      .map((f) => f.tag);
    // stop 释放了 gate（engine.stopped=true）→ 不再有 echo（gate 在 emit 前），
    // 但必须有 [STOPPED] 与 [END_OF_RESPONSE]
    expect(tags).toContain("[STOPPED]");
    expect(tags).toContain("[END_OF_RESPONSE]");
    expect(tags).not.toContain("[AIMSG]");
  });

  test("close 帧 → 会话被回收，后续 user 帧被丢弃", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    core.handleLine(
      JSON.stringify({ v: 1, type: "create", id: 1, sid: SID, workspace: newWorkspace() }),
    );
    await settle();
    core.handleLine(JSON.stringify({ v: 1, type: "close", sid: SID }));
    await settle();

    expect(core.sessionCount()).toBe(0);
    expect(created[0]!.record.disposeCalls).toBe(1);
    const eventsBefore = frames(out).filter((f) => f.type === "event").length;

    core.handleLine(JSON.stringify({ v: 1, type: "user", sid: SID, data: "late msg" }));
    await settle();
    expect(frames(out).filter((f) => f.type === "event").length).toBe(eventsBefore);
  });

  test("同 sid 重建：旧引擎被 stop+dispose，注册表只剩新会话", async () => {
    const created: FakeEngine[] = [];
    const { core } = makeCore(created);
    const ws = newWorkspace();
    core.handleLine(JSON.stringify({ v: 1, type: "create", id: 1, sid: SID, workspace: ws }));
    await settle();
    core.handleLine(JSON.stringify({ v: 1, type: "create", id: 2, sid: SID, workspace: ws }));
    await settle();

    expect(core.sessionCount()).toBe(1);
    expect(created).toHaveLength(2);
    expect(created[0]!.record.stopCalls).toBe(1);
    expect(created[0]!.record.disposeCalls).toBe(1);
    expect(created[1]!.record.disposeCalls).toBe(0);
  });

  test("create 失败 → err 应答携带错误信息，无会话残留", async () => {
    const out: string[] = [];
    const boom = async (): Promise<EngineAdapter> => {
      throw new Error("Claude CLI not found: /no/such/cli");
    };
    const core = new DaemonCore({
      write: (l) => out.push(l),
      createEngineFn: boom as unknown as DaemonOptions["createEngineFn"],
    });
    core.handleLine(
      JSON.stringify({ v: 1, type: "create", id: 9, sid: SID, workspace: newWorkspace() }),
    );
    await settle();

    expect(frames(out)).toEqual([
      { v: 1, id: 9, type: "err", error: "Claude CLI not found: /no/such/cli" },
    ]);
    expect(core.sessionCount()).toBe(0);
  });

  test("workspace 不存在 → err 应答", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    core.handleLine(
      JSON.stringify({ v: 1, type: "create", id: 3, sid: SID, workspace: "/no/such/dir" }),
    );
    await settle();

    expect(frames(out)).toEqual([{ v: 1, id: 3, type: "err", error: "workspace not found: /no/such/dir" }]);
    expect(core.sessionCount()).toBe(0);
  });

  test("非 JSON 行与不合帧行被忽略，不影响后续路由", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    core.handleLine("some stray console.log noise");
    core.handleLine('{"foo": 1}');
    core.handleLine(
      JSON.stringify({ v: 1, type: "create", id: 1, sid: SID, workspace: newWorkspace() }),
    );
    await settle();

    expect(frames(out)).toEqual([{ v: 1, id: 1, type: "ok" }]);
  });

  test("未知 sid 的 user/stop 帧被静默丢弃", async () => {
    const created: FakeEngine[] = [];
    const { core, out } = makeCore(created);
    core.handleLine(JSON.stringify({ v: 1, type: "user", sid: "ghost", data: "x" }));
    core.handleLine(JSON.stringify({ v: 1, type: "stop", sid: "ghost" }));
    core.handleLine(JSON.stringify({ v: 1, type: "close", sid: "ghost" }));
    await settle();

    expect(out).toEqual([]);
  });
});
