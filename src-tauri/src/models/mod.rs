use serde::{Deserialize, Serialize};

pub mod db;
pub use db::{init_pool, DbPool};

/// 目录组（邻接表 + 路径枚举）
#[allow(non_snake_case)]
#[derive(Debug, Default, sqlx::FromRow, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    /// 路径枚举，如 "/1/3/" 表示该节点祖先链；根节点为 "/"
    pub path: String,
    pub level: i64,
    pub is_del: i64,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GroupCreate {
    pub parent_id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GroupUpdate {
    pub name: Option<String>,
    pub parent_id: Option<i64>,
}

/// 主机
#[allow(non_snake_case)]
#[derive(Debug, Default, sqlx::FromRow, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: i64,
    pub gid: i64,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub addr: String,
    pub port: String,
    pub username: String,
    /// 数据库中以 AES-GCM 密文存储；接口出参隐藏
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub desc: Option<String>,
    pub is_del: i64,
    /// 数据库中以 AES-GCM 密文存储；接口出参隐藏
    #[serde(skip_serializing)]
    pub private_key: Option<String>,
    /// 私钥文件路径（明文存储）；SSH 连接时由后端读取文件内容
    pub private_key_path: Option<String>,
    /// 连接协议：ssh（默认）/ telnet / serial
    pub protocol: String,
    /// 串口波特率（仅 protocol=serial）
    pub baud_rate: Option<i64>,
    /// 串口数据位（仅 protocol=serial）
    pub data_bits: Option<i64>,
    /// 串口停止位（仅 protocol=serial）
    pub stop_bits: Option<i64>,
    /// 串口校验：none/odd/even（仅 protocol=serial）
    pub parity: Option<String>,
    /// 串口流控：none/software/hardware（仅 protocol=serial）
    pub flow_control: Option<String>,
    /// SSH keepalive 间隔（秒），0 或 null 使用默认 30s
    pub keepalive_interval: Option<i64>,
    /// SSH 不活动超时（秒），0 或 null 使用默认 120s
    pub inactivity_timeout: Option<i64>,
    /// 终端 idle 时定时发送空字符的间隔（秒），0 或 null 表示不发送
    pub idle_send_interval: Option<i64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HostCreate {
    pub gid: i64,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub addr: String,
    pub port: String,
    pub username: String,
    pub password: Option<String>,
    pub desc: Option<String>,
    pub private_key: Option<String>,
    pub private_key_path: Option<String>,
    /// 连接协议：ssh（默认）/ telnet / serial
    pub protocol: Option<String>,
    pub baud_rate: Option<i64>,
    pub data_bits: Option<i64>,
    pub stop_bits: Option<i64>,
    pub parity: Option<String>,
    pub flow_control: Option<String>,
    pub keepalive_interval: Option<i64>,
    pub inactivity_timeout: Option<i64>,
    pub idle_send_interval: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct HostUpdate {
    pub gid: Option<i64>,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub addr: Option<String>,
    pub port: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub desc: Option<String>,
    pub private_key: Option<String>,
    pub private_key_path: Option<String>,
    pub protocol: Option<String>,
    pub baud_rate: Option<i64>,
    pub data_bits: Option<i64>,
    pub stop_bits: Option<i64>,
    pub parity: Option<String>,
    pub flow_control: Option<String>,
    pub keepalive_interval: Option<i64>,
    pub inactivity_timeout: Option<i64>,
    pub idle_send_interval: Option<i64>,
}

/// 列表联表 DTO：包含 host 全字段 + 所属 group 名称 / 上级 gid
#[allow(non_snake_case, dead_code)]
#[derive(Debug, Default, sqlx::FromRow, Clone, Serialize, Deserialize)]
pub struct HostWithGroup {
    pub id: i64,
    pub gid: i64,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub addr: String,
    pub port: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub desc: Option<String>,
    pub is_del: i64,
    #[serde(skip_serializing)]
    pub private_key: Option<String>,
    pub private_key_path: Option<String>,
    pub protocol: String,
    pub baud_rate: Option<i64>,
    pub data_bits: Option<i64>,
    pub stop_bits: Option<i64>,
    pub parity: Option<String>,
    pub flow_control: Option<String>,
    pub keepalive_interval: Option<i64>,
    pub inactivity_timeout: Option<i64>,
    pub idle_send_interval: Option<i64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub group_name: Option<String>,
    pub parent_gid: Option<i64>,
}

/// AI 供应商
#[allow(non_snake_case)]
#[derive(Debug, Default, sqlx::FromRow, Clone, Serialize, Deserialize)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    /// API 类型：openai-completions / anthropic-messages / openai-responses / google-generative-ai
    pub api_type: String,
    pub base_url: String,
    /// API Key（数据库 AES-GCM 加密）
    pub api_key: String,
    /// 候选模型 ID（逗号分隔）
    pub model_ids: String,
    pub sort_order: i64,
    pub is_del: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Sidecar 引擎配置：每个引擎（claude / pi）一行
#[derive(Debug, Default, sqlx::FromRow, Clone, Serialize, Deserialize)]
pub struct AiEngine {
    pub engine: String,
    pub provider_id: Option<String>,
    pub active_model_id: String,
    /// Thinking Level（仅 pi 引擎使用）
    pub thinking_level: String,
    pub updated_at: Option<String>,
}

/// GET /api/ai-engines 的响应：当前激活引擎 + 全部引擎配置
#[derive(Debug, Serialize)]
pub struct AiEnginesState {
    pub active_engine: String,
    pub engines: Vec<AiEngine>,
}

#[derive(Debug, Deserialize)]
pub struct AiProviderCreate {
    pub name: String,
    pub api_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model_ids: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AiProviderUpdate {
    pub name: Option<String>,
    pub api_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model_ids: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AiEngineUpdate {
    pub provider_id: Option<String>,
    pub active_model_id: Option<String>,
    pub thinking_level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AiEngineActivate {
    pub engine: String,
}

/// AI 常用语（收藏的用户消息）
#[derive(Debug, Default, sqlx::FromRow, Clone, Serialize, Deserialize)]
pub struct QuickPhrase {
    pub id: i64,
    pub content: String,
    pub sort_order: i64,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QuickPhraseCreate {
    pub content: String,
}

/// 模板命令（预置命令片段）
#[derive(Debug, Default, sqlx::FromRow, Clone, Serialize, Deserialize)]
pub struct CommandTemplate {
    pub id: i64,
    pub title: String,
    pub command: String,
    pub description: Option<String>,
    pub sort_order: i64,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommandTemplateCreate {
    pub title: String,
    pub command: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommandTemplateUpdate {
    pub title: Option<String>,
    pub command: Option<String>,
    pub description: Option<String>,
}
