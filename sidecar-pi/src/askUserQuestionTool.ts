import { defineTool } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

/**
 * 创建 AskUserQuestion 工具。
 *
 * 参照 sidecar-cc/src/index.ts 中 canUseTool 对 AskUserQuestion 的处理逻辑：
 * - 通过 stdout [AI_ASKUSERQUESTION] 标记将问题发给前端
 * - 通过 stdin 等待用户选择/输入
 * - 返回答案给 LLM
 *
 * readLineFromStdin 由 index.ts 注入，复用其 stdin 缓冲机制。
 */
export function createAskUserQuestionTool(readLineFromStdin: () => Promise<string>) {
  return defineTool({
    name: "AskUserQuestion",
    label: "Ask User Question",
    description:
      "Ask the user a question and let them pick from options. Use when you need user input to proceed.",
    promptSnippet: "AskUserQuestion: ask the user a question with selectable options",
    parameters: Type.Object({
      questions: Type.Array(
        Type.Object({
          header: Type.String({ description: "Very short label (max 12 chars)" }),
          question: Type.String({ description: "The question to ask the user" }),
          options: Type.Array(
            Type.Object({
              label: Type.String({ description: "Display label for the option" }),
              description: Type.Optional(
                Type.String({ description: "Optional explanation shown below label" }),
              ),
            }),
            { description: "Available options for the user to choose from" },
          ),
          multiSelect: Type.Optional(
            Type.Boolean({ description: "Allow multiple selections" }),
          ),
        }),
        { description: "Questions to ask the user" },
      ),
    }),
    executionMode: "sequential",

    async execute(_toolCallId, params, _signal, _onUpdate, _ctx) {
      const answers: Record<string, string> = {};

      for (const q of params.questions) {
        const title = `${q.header}: ${q.question}`;
        const options = q.options;
        const items: string[] = [];

        options.forEach((opt, i) => {
          items.push(`${i + 1}. ${opt.label}${opt.description ? ` - ${opt.description}` : ""}`);
        });

        const tips = q.multiSelect
          ? "(Enter numbers separated by commas, or type your own answer)"
          : "(Enter a number, or type your own answer)";

        const aiMsg = { type: "AskUserQuestion", payload: { title, items, tips } };
        console.log(`[AI_ASKUSERQUESTION]${JSON.stringify(aiMsg)}`);

        const response = (await readLineFromStdin()).trim();

        if (q.multiSelect) {
          const indices = response.split(",").map((s) => parseInt(s.trim()) - 1);
          const labels = indices
            .filter((i) => !isNaN(i) && i >= 0 && i < options.length)
            .map((i) => options[i]!.label);
          answers[q.question] = labels.length > 0 ? labels.join(", ") : response;
        } else {
          const index = parseInt(response) - 1;
          if (!isNaN(index) && index >= 0 && index < options.length) {
            answers[q.question] = options[index]!.label;
          } else {
            answers[q.question] = response;
          }
        }
      }

      const answerText = Object.entries(answers)
        .map(([q, a]) => `${q}: ${a}`)
        .join("\n");

      return {
        content: [{ type: "text" as const, text: answerText }],
        details: { answers },
      };
    },
  });
}
