import { defineTool } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import type { TodoItem } from "../../progress";

const TodoStatus = Type.Union(
  [Type.Literal("pending"), Type.Literal("in_progress"), Type.Literal("completed")],
  { description: "Status of the todo item" },
);

/**
 * TodoWrite 工具：LLM 提交 todos 数组，经 onUpdate 回调交给 index.ts
 * 存储并由 displayTodoProgress 推送进度（[AIMSG]），返回确认信息给 LLM。
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
              description:
                "Present-tense description of what you're currently doing (for in_progress items)",
            }),
          ),
        }),
        { description: "The full todo list (replaces previous list)" },
      ),
    }),
    executionMode: "sequential",

    async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
      onUpdate(params.todos as TodoItem[]);

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
