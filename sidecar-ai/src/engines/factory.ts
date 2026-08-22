import type { EngineAdapter, EngineContext } from "./types";

export type EngineType = "claude" | "pi";

/**
 * 引擎工厂：按类型构造对应的适配器。
 * 未知类型回退 claude（与 Rust 端 spawn_sidecar 的默认行为一致）。
 *
 * 引擎模块走动态 import：只有选中的 SDK 会初始化，另一个引擎不付
 * 启动成本、常驻内存和模块级故障半径。
 */
export async function createEngine(
  type: string,
  ctx: EngineContext,
): Promise<EngineAdapter> {
  switch (type) {
    case "pi":
      return (await import("./pi")).createPiEngine(ctx);
    case "claude":
      return (await import("./claude")).createClaudeEngine(ctx);
    default:
      console.warn(`[FACTORY] Unknown engine type '${type}', falling back to 'claude'`);
      return (await import("./claude")).createClaudeEngine(ctx);
  }
}
