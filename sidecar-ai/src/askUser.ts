import { emitAskUserQuestion } from "./protocol";
import { STOP_SENTINEL, type StdinIO } from "./stdin";

export interface AskUserQuestionItem {
  header: string;
  question: string;
  options: { label: string; description?: string }[];
  multiSelect?: boolean;
}

/**
 * 把一道选择题发给前端（[AI_ASKUSERQUESTION]）并等待用户作答。
 * 返回答案文本：命中的选项 label（多选用 ", " 连接）或用户原始输入；
 * 返回 null 表示被 __STOP__ 打断。
 *
 * claude 的 canUseTool 分支与 pi 的自定义工具共用此实现。
 */
export async function askUserQuestion(
  io: StdinIO,
  q: AskUserQuestionItem,
): Promise<string | null> {
  const items = q.options.map(
    (opt, i) => `${i + 1}. ${opt.label}${opt.description ? ` - ${opt.description}` : ""}`,
  );
  const tips = q.multiSelect
    ? "(Enter numbers separated by commas, or type your own answer)"
    : "(Enter a number, or type your own answer)";

  emitAskUserQuestion({ title: `${q.header}: ${q.question}`, items, tips });

  const response = (await io.readLineOrStop()).trim();
  if (response === STOP_SENTINEL) return null;

  const indices = response.split(",").map((s) => parseInt(s.trim(), 10) - 1);
  const labels = indices
    .filter((i) => !isNaN(i) && i >= 0 && i < q.options.length)
    .map((i) => q.options[i]!.label);
  return labels.length > 0 ? labels.join(", ") : response;
}
