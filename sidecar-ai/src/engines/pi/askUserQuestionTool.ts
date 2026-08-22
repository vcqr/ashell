import { defineTool } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { askUserQuestion } from "../../askUser";
import type { StdinIO } from "../../stdin";

/**
 * AskUserQuestion 工具：通过共享的 askUserQuestion 核心
 * （[AI_ASKUSERQUESTION] 发题 + stdin 等待作答）把问题交给用户选择。
 * io 由 index.ts 注入，复用全局 stdin 缓冲与停止信号机制。
 */
export function createAskUserQuestionTool(io: StdinIO) {
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
      let interrupted = false;

      for (const q of params.questions) {
        const answer = await askUserQuestion(io, q);
        if (answer === null) {
          interrupted = true;
          break;
        }
        answers[q.question] = answer;
      }

      if (interrupted) {
        return {
          content: [{ type: "text" as const, text: "用户已中断" }],
          details: { interrupted: true },
        };
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
