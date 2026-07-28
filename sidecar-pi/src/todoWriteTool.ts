import { defineTool } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

const TodoStatus = Type.Union(
  [Type.Literal("pending"), Type.Literal("in_progress"), Type.Literal("completed")],
  { description: "Status of the todo item" },
);

/**
 * 创建 TodoWrite 工具。
 *
 * 参照 sidecar-cc/src/index.ts 中对 TodoWrite 的处理：
 * - LLM 调用 TodoWrite 传入 todos 数组
 * - 通过 onUpdate 回调将 todos 传给 index.ts，由 displayProgress() 发送 [AIMSG]
 * - 返回确认信息给 LLM
 *
 * onUpdate 由 index.ts 注入，负责存储 todos 并向前端输出进度。
 */
export function createTodoWriteTool(onUpdate: (todos: TodoItem[]) => void) {
  return defineTool({
    name: "TodoWrite",
    label: "Todo Write",
    description:
      "Create or update a structured task list to track progress on complex multi-step work. " +
      "Each item has a status (pending, in_progress, completed). " +
      "Use this proactively for tasks with 3+ steps.",
    promptSnippet: "TodoWrite: manage a structured todo list to track task progress",
    parameters: Type.Object({
      todos: Type.Array(
        Type.Object({
          content: Type.String({ description: "Description of the task" }),
          status: TodoStatus,
          activeForm: Type.Optional(
            Type.String({
              description: "Present-tense description of what you're currently doing (for in_progress items)",
            }),
          ),
        }),
        { description: "The full todo list (replaces previous list)" },
      ),
    }),
    executionMode: "sequential",

    async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
      onUpdate(params.todos);

      const completed = params.todos.filter((t) => t.status === "completed").length;
      const total = params.todos.length;

      return {
        content: [
          {
            type: "text" as const,
            text: `Todos updated. ${completed}/${total} completed.`,
          },
        ],
        details: { count: total, completed },
      };
    },
  });
}

export interface TodoItem {
  content: string;
  status: "pending" | "in_progress" | "completed";
  activeForm?: string;
}
