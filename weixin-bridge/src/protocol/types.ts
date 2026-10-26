/**
 * 微信 iLink 机器人协议类型（JSON over HTTP，bytes 字段为 base64 字符串）。
 * 类型与字段名对齐 @tencent-weixin/openclaw-weixin 的 src/api/types.ts。
 */

/** 每个请求附带的公共元数据。 */
export interface BaseInfo {
  channel_version?: string;
  /** 仅用于观测（日志归因），不参与鉴权或路由。 */
  bot_agent?: string;
}

export const MessageType = {
  NONE: 0,
  USER: 1,
  BOT: 2,
} as const;

export const MessageItemType = {
  NONE: 0,
  TEXT: 1,
  IMAGE: 2,
  VOICE: 3,
  FILE: 4,
  VIDEO: 5,
} as const;

export const MessageState = {
  NEW: 0,
  GENERATING: 1,
  FINISH: 2,
} as const;

export interface TextItem {
  text?: string;
}

export interface MessageItem {
  type?: number;
  create_time_ms?: number;
  update_time_ms?: number;
  is_completed?: boolean;
  msg_id?: string;
  text_item?: TextItem;
}

/** 统一消息结构（proto: WeixinMessage）。 */
export interface WeixinMessage {
  seq?: number;
  /** 线上为 uint64，这里以字符串无损保存。 */
  message_id?: string;
  from_user_id?: string;
  to_user_id?: string;
  client_id?: string;
  create_time_ms?: number;
  update_time_ms?: number;
  session_id?: string;
  group_id?: string;
  message_type?: number;
  message_state?: number;
  item_list?: MessageItem[];
  /** 服务端按消息下发的会话上下文令牌，回复时透传可提高可达性。 */
  context_token?: string;
  run_id?: string;
}

export interface GetUpdatesReq {
  /** 本地缓存的上下文 buf；首次或重置后传 ""。 */
  get_updates_buf?: string;
}

export interface GetUpdatesResp {
  ret?: number;
  /** 服务端错误码（如 -14 = token 失效）。 */
  errcode?: number;
  errmsg?: string;
  msgs?: WeixinMessage[];
  /** 下次请求需原样携带并本地持久化。 */
  get_updates_buf?: string;
  /** 服务端建议的下一次长轮询超时（毫秒）。 */
  longpolling_timeout_ms?: number;
}

export interface SendMessageReq {
  msg?: WeixinMessage;
}

export interface SendMessageResp {
  message_id?: string;
  ret?: number;
  /** 后端在 HTTP 200 下也用 errcode 报错（如 -14 = session timeout）。 */
  errcode?: number;
  errmsg?: string;
}

export interface NotifyStopResp {
  ret?: number;
  errmsg?: string;
}

export interface NotifyStartResp {
  ret?: number;
  errmsg?: string;
}
