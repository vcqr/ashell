import { emitAIMSG } from "./protocol";

export interface TodoItem {
  content: string;
  status: "pending" | "in_progress" | "completed";
  activeForm?: string;
}

/** TodoWrite 进度推送（两个引擎共用同一渲染） */
export function displayTodoProgress(todos: TodoItem[]): void {
  if (todos.length === 0) return;

  const completed = todos.filter((t) => t.status === "completed").length;
  const inProgress = todos.filter((t) => t.status === "in_progress").length;
  const total = todos.length;

  let msg = `Progress: ${completed}/${total} completed \n Currently working on: ${inProgress} task(s)`;
  todos.forEach((todo, index) => {
    const icon =
      todo.status === "completed" ? "✅" : todo.status === "in_progress" ? "🔧" : "❌";
    const text = todo.status === "in_progress" ? todo.activeForm : todo.content;
    msg += `\n${index + 1}. ${icon} ${text}`;
  });

  emitAIMSG(msg);
}
