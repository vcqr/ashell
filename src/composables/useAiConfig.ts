import { invoke } from "@tauri-apps/api/core";

export type SidecarType = "claude" | "pi";

export type AiModelConfig = {
  url: string;
  key: string;
  modelIds: string;
  activeModelId: string;
  sidecarType: string;
  piProvider: string;
  piModel: string;
  piModelIds: string;
  piBaseUrl: string;
  piApiKey: string;
  piApi: string;
  piThinkingLevel: string;
};

export function emptyModelConfig(): AiModelConfig {
  return {
    url: "",
    key: "",
    modelIds: "",
    activeModelId: "",
    sidecarType: "",
    piProvider: "",
    piModel: "",
    piModelIds: "",
    piBaseUrl: "",
    piApiKey: "",
    piApi: "",
    piThinkingLevel: "",
  };
}

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

export const piApiOptions = [
  { label: "OpenAI Completions", value: "openai-completions" },
  { label: "Anthropic Messages", value: "anthropic-messages" },
  { label: "OpenAI Responses", value: "openai-responses" },
  { label: "Google Generative AI", value: "google-generative-ai" },
];

export const piThinkingLevelOptions = [
  { label: "Off", value: "off" },
  { label: "Minimal", value: "minimal" },
  { label: "Low", value: "low" },
  { label: "Medium", value: "medium" },
  { label: "High", value: "high" },
  { label: "XHigh", value: "xhigh" },
  { label: "Max", value: "max" },
];

/**
 * 根据供应商的 sidecarType / piApi 推断 fetch_models 需要的 api_type。
 * - claude sidecar → "anthropic"
 * - pi sidecar + anthropic-messages → "anthropic"
 * - pi sidecar + openai-completions / openai-responses → "openai"
 * - pi sidecar + google-generative-ai → "google"
 */
export function inferApiType(sidecarType: string, piApi: string): string {
  if (sidecarType !== "pi") return "anthropic";
  switch (piApi) {
    case "anthropic-messages":
      return "anthropic";
    case "google-generative-ai":
      return "google";
    default:
      return "openai";
  }
}

/**
 * 获取供应商对应的 base_url 和 api_key（用于 fetch_models）
 */
export function getProviderEndpoint(
  sidecarType: string,
  url: string,
  piBaseUrl: string,
  apiKey: string,
  piApiKey: string,
): { baseUrl: string; apiKey: string } {
  if (sidecarType === "pi") {
    return { baseUrl: piBaseUrl, apiKey: piApiKey };
  }
  return { baseUrl: url, apiKey };
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
