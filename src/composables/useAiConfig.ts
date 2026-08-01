import { invoke } from "@tauri-apps/api/core";

export type SidecarType = "claude" | "pi";

export function parseModelIds(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

export function normalizeModelIds(value: string): string {
  return parseModelIds(value).join(", ");
}

export function resolveActiveModelId(
  modelIds: string,
  activeModelId: string,
): string {
  const list = parseModelIds(modelIds);
  return list.includes(activeModelId) ? activeModelId : (list[0] ?? "");
}

export const sidecarTypeOptions = [
  { label: "Claude Agent SDK", value: "claude" },
  { label: "Pi Coding Agent", value: "pi" },
];

export const apiTypeOptions = [
  { label: "OpenAI Completions", value: "openai-completions" },
  { label: "Anthropic Messages", value: "anthropic-messages" },
  { label: "OpenAI Responses", value: "openai-responses" },
  { label: "Google Generative AI", value: "google-generative-ai" },
];

export const thinkingLevelOptions = [
  { label: "Off", value: "off" },
  { label: "Minimal", value: "minimal" },
  { label: "Low", value: "low" },
  { label: "Medium", value: "medium" },
  { label: "High", value: "high" },
  { label: "XHigh", value: "xhigh" },
  { label: "Max", value: "max" },
];

/**
 * 根据供应商的 api_type 推断 fetch_models 需要的协议族。
 * - anthropic-messages → "anthropic"
 * - google-generative-ai → "google"
 * - openai-completions / openai-responses → "openai"
 */
export function inferFetchApiType(apiType: string): string {
  switch (apiType) {
    case "anthropic-messages":
      return "anthropic";
    case "google-generative-ai":
      return "google";
    default:
      return "openai";
  }
}

/**
 * 调用后端 fetch_models 命令获取模型列表
 */
export async function fetchModelList(
  baseUrl: string,
  apiKey: string,
  apiType: string,
): Promise<string[]> {
  return invoke<string[]>("fetch_models", { baseUrl, apiKey, apiType });
}
