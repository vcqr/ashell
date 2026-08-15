import axios, { AxiosError, AxiosHeaders } from "axios"
import type {
  AxiosInstance,
  AxiosProgressEvent,
  AxiosRequestConfig,
  AxiosResponse,
  GenericAbortSignal,
  ResponseType,
} from "axios"
import { invoke } from "@tauri-apps/api/core"
import type { ApiInfo } from "@/types"

/** 后端统一返回包装 */
export interface ApiEnvelope<T> {
  code: number
  message: string
  data?: T
}

/** 调用失败时抛出的错误 */
export class ApiError extends Error {
  code: number
  status?: number
  constructor(code: number, message: string, status?: number) {
    super(message)
    this.code = code
    this.status = status
  }
}

let apiInfoPromise: Promise<ApiInfo> | null = null

/** 获取并缓存 ApiInfo（懒加载，启动后第一次调用阻塞等待 backend ready） */
export async function getApiInfo(): Promise<ApiInfo> {
  if (!apiInfoPromise) {
    apiInfoPromise = (async () => {
      let lastErr: unknown = null
      for (let i = 0; i < 30; i++) {
        try {
          const info = await invoke<ApiInfo>("get_api_info")
          if (info && info.base_url) return info
        } catch (e) {
          lastErr = e
        }
        await new Promise((r) => setTimeout(r, 100))
      }
      apiInfoPromise = null
      throw new ApiError(-1, `failed to get api info: ${String(lastErr ?? "timeout")}`)
    })()
  }
  return apiInfoPromise
}

/** axios 实例：base_url + token 由请求拦截器在每次请求时注入 */
let axiosInstance: AxiosInstance | null = null

export function getAxios(): AxiosInstance {
  if (axiosInstance) return axiosInstance
  const ins = axios.create({
    timeout: 30_000,
  })

  ins.interceptors.request.use(async (config) => {
    const info = await getApiInfo()
    if (!config.baseURL) config.baseURL = info.base_url
    const headers = AxiosHeaders.from(config.headers ?? {})
    if (!headers.has("Authorization")) {
      headers.set("Authorization", `Bearer ${info.token}`)
    }
    config.headers = headers
    return config
  })

  ins.interceptors.response.use(
    (resp) => resp,
    (err: AxiosError) => {
      // 取消请求
      if (axios.isCancel(err) || err.code === "ERR_CANCELED") {
        return Promise.reject(
          new DOMException(err.message || "canceled", "AbortError"),
        )
      }
      const status = err.response?.status
      const data = err.response?.data as
        | { code?: number; message?: string }
        | undefined
      const message =
        data?.message ?? err.message ?? (status ? `HTTP ${status}` : "request failed")
      return Promise.reject(new ApiError(data?.code ?? status ?? -1, message, status))
    },
  )

  axiosInstance = ins
  return ins
}

interface RequestOptions {
  /** HTTP method */
  method?: AxiosRequestConfig["method"]
  /** query 参数；值为 undefined/null 自动忽略 */
  params?: Record<string, string | number | boolean | undefined | null>
  /** JSON body */
  json?: unknown
  /** 直接传 body（multipart 等） */
  body?: unknown
  /** 取消信号 */
  signal?: GenericAbortSignal
  /** 请求超时（毫秒）；0 = 不超时，缺省走实例默认 30s（大文件传输需显式传 0） */
  timeout?: number
  /** 自定义 headers */
  headers?: Record<string, string>
  /** 仅在 raw 模式下使用：响应类型 */
  responseType?: ResponseType
  /** 上传进度（仅对 raw 模式 / requestRaw 有意义） */
  onUploadProgress?: (e: AxiosProgressEvent) => void
  /** 下载进度（仅对 raw 模式有意义） */
  onDownloadProgress?: (e: AxiosProgressEvent) => void
}

function cleanParams(
  params: RequestOptions["params"],
): Record<string, string | number | boolean> | undefined {
  if (!params) return undefined
  const out: Record<string, string | number | boolean> = {}
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null) continue
    out[k] = v
  }
  return out
}

function buildAxiosConfig(
  path: string,
  opts: RequestOptions,
  override: Partial<AxiosRequestConfig> = {},
): AxiosRequestConfig {
  const data = opts.json !== undefined ? opts.json : opts.body
  return {
    url: path,
    method: opts.method ?? "GET",
    params: cleanParams(opts.params),
    data,
    headers: opts.headers,
    signal: opts.signal,
    timeout: opts.timeout,
    onUploadProgress: opts.onUploadProgress,
    onDownloadProgress: opts.onDownloadProgress,
    ...override,
  }
}

/** 标准 JSON 请求：自动解包 envelope */
export async function request<T = unknown>(
  path: string,
  opts: RequestOptions = {},
): Promise<T> {
  const ins = getAxios()
  const resp = await ins.request<ApiEnvelope<T>>(buildAxiosConfig(path, opts))
  const env = resp.data
  if (env == null || typeof env !== "object" || !("code" in env)) {
    throw new ApiError(-1, "invalid response envelope", resp.status)
  }
  if (env.code !== 0) {
    throw new ApiError(env.code, env.message ?? "request failed", resp.status)
  }
  return env.data as T
}

/** 原始请求：返回 axios Response，调用方自行处理（流式下载等） */
export async function requestRaw<T = unknown>(
  path: string,
  opts: RequestOptions = {},
): Promise<AxiosResponse<T>> {
  const ins = getAxios()
  return ins.request<T>(
    buildAxiosConfig(path, opts, {
      responseType: opts.responseType ?? "blob",
    }),
  )
}

/** 构造带 token 的 WebSocket URL */
export async function buildWsUrl(
  path: string,
  query: Record<string, string | number | boolean | undefined | null> = {},
): Promise<string> {
  const info = await getApiInfo()
  const sp = new URLSearchParams()
  sp.append("token", info.token)
  for (const [k, v] of Object.entries(query)) {
    if (v === undefined || v === null) continue
    sp.append(k, String(v))
  }
  return `${info.ws_url}${path}?${sp.toString()}`
}
