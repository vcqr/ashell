/* ---------- 后端 DTO（与 src-tauri/src/models 对应） ---------- */

/** 目录组 */
export interface Group {
  id: number
  parent_id: number
  name: string
  /** 路径枚举，如 "/1/3/" */
  path: string
  level: number
  is_del: number
  created_at?: string | null
  updated_at?: string | null
}

export interface GroupCreate {
  parent_id: number
  name: string
}

export interface GroupUpdate {
  name?: string
  parent_id?: number
}

/** 连接协议 */
export type HostProtocol = 'ssh' | 'telnet' | 'serial'

/** 主机（不含密文字段） */
export interface Host {
  id: number
  gid: number
  name: string
  icon?: string | null
  color?: string | null
  addr: string
  /** 后端 port 是 string */
  port: string
  username: string
  desc?: string | null
  is_del: number
  /** 私钥文件路径（明文存储，前端可见） */
  private_key_path?: string | null
  /** 连接协议：ssh（默认）/ telnet / serial */
  protocol: HostProtocol
  /** 串口波特率（仅 protocol=serial） */
  baud_rate?: number | null
  /** 串口数据位（仅 protocol=serial） */
  data_bits?: number | null
  /** 串口停止位（仅 protocol=serial） */
  stop_bits?: number | null
  /** 串口校验：none/odd/even（仅 protocol=serial） */
  parity?: string | null
  /** 串口流控：none/software/hardware（仅 protocol=serial） */
  flow_control?: string | null
  created_at?: string | null
  updated_at?: string | null
}

/** 联表查询返回（with_group=1） */
export interface HostWithGroup extends Host {
  group_name?: string | null
  parent_gid?: number | null
}

export interface HostCreate {
  gid: number
  name: string
  icon?: string | null
  color?: string | null
  addr: string
  port: string
  username: string
  password?: string | null
  desc?: string | null
  private_key?: string | null
  private_key_path?: string | null
  protocol?: HostProtocol
  baud_rate?: number | null
  data_bits?: number | null
  stop_bits?: number | null
  parity?: string | null
  flow_control?: string | null
}

export interface HostUpdate {
  gid?: number
  name?: string
  icon?: string | null
  color?: string | null
  addr?: string
  port?: string
  username?: string
  password?: string | null
  desc?: string | null
  private_key?: string | null
  private_key_path?: string | null
  protocol?: HostProtocol
  baud_rate?: number | null
  data_bits?: number | null
  stop_bits?: number | null
  parity?: string | null
  flow_control?: string | null
}

/** 从 ~/.ssh/config 解析出的主机条目 */
export interface SshConfigHost {
  name: string
  addr: string
  port: string
  username?: string | null
  identity_file?: string | null
}

/* ---------- SFTP DTO ---------- */

export interface SftpFile {
  file_name: string
  /** "dir" | "file" | "symlink" | "other" */
  file_type: string
  full_path: string
  link_path?: string | null
  /** 人类可读大小，如 "12.34 K" */
  size: string
  size_bytes?: number
  user: string
  group: string
  permissions: string
  /** Unix 秒级时间戳 */
  atime?: number | null
  mtime?: number | null
}

export interface SftpListResp {
  sid: string
  path: string
  files: SftpFile[]
}

/** 上传 / 下载任务的 UI 状态 */
export type TransferStatus = 'pending' | 'running' | 'done' | 'error' | 'cancelled'

export interface TransferTask {
  id: string
  sid: string
  /** 远端路径或本地文件名 */
  filename: string
  /** 远端目录（上传时） */
  remoteDir?: string
  /** 字节 */
  total: number
  loaded: number
  status: TransferStatus
  error?: string
  controller?: AbortController
  startedAt: number
}

/* ---------- 前端运行时类型 ---------- */

/** 主机树节点（前端展示用） */
export interface HostNode {
  /** 唯一 key（folder-{id} / host-{id}） */
  key: string
  label: string
  type: 'folder' | 'host'
  /** group / host 的真实 id（folder=group.id, host=host.id） */
  id: number
  /** host 节点附带的字段 */
  host?: string
  port?: string
  username?: string
  icon?: string | null
  color?: string | null
  desc?: string | null
  gid?: number
  /** 连接协议：ssh / telnet / serial */
  protocol?: HostProtocol
  /** folder 节点附带的字段 */
  parentId?: number
  level?: number
  children?: HostNode[]
}

/** 终端 Tab */
export interface TerminalTab {
  key: string
  title: string
  /** tab 类型；省略视为 'ssh'（兼容旧持久化数据） */
  kind?: 'ssh' | 'local' | 'telnet' | 'serial'
  /** 关联的主机 id（仅远程 tab 有） */
  hostId?: number
  /** 后端会话 id（来自 WebSocket 首帧 ready） */
  sid?: string
  /** 主机节点 key（用于在树中定位） */
  hostKey?: string
  /** host.icon 文件名（来自 ~/.ashell/icons），用于 tab 与树中显示自定义图标 */
  icon?: string | null
  /** 主机自定义颜色（hex，如 "#7c5cff"），用于 tab 激活态着色 */
  color?: string | null
  /** 连接状态 */
  status?: 'connecting' | 'connected' | 'closed' | 'error'
  /** 本地 PTY tab 使用的 shell 名（powershell/pwsh/cmd/bash/zsh/...）。 */
  shell?: string | null
  /** 主机展示信息（断线重连时复用） */
  hostInfo?: {
    addr: string
    port: string
    username: string
  }
}

export interface ProcessStep {
  type: 'tool_call' | 'tool_ret'
  /** 已渲染好的 markdown 片段 */
  content: string
  /** 仅 tool_call 有；用于折叠态预览 */
  toolName?: string
  time: string
}

export interface ChatMessage {
  id: number
  role: 'user' | 'assistant'
  content: string
  time: string
  isStreaming?: boolean
  /** 标记为"中间过程"消息：聚合 AITOOL/TOOL_RET，渲染为可折叠块 */
  isProcess?: boolean
  processSteps?: ProcessStep[]
  /** 标记为 thinking 消息：渲染为可折叠的思考过程块 */
  thinking?: boolean
}

/* ---------- 主机图标资源（/api/icons） ---------- */

export interface IconItem {
  /** 文件名（含扩展名），即 host.icon 字段 */
  name: string
  /** MIME，如 image/png */
  mime: string
  size: number
  /** Unix 秒级修改时间，用于前端缓存失效（URL 拼 ?v=mtime） */
  mtime: number
}

/* ---------- API 信息（来自 invoke("get_api_info")） ---------- */

export interface ApiInfo {
  addr: string
  token: string
  base_url: string
  ws_url: string
}

/* ---------- 主机系统信息（/api/ssh/sysinfo） ---------- */

export interface DiskUsage {
  filesystem: string
  mount: string
  total_bytes: number
  used_bytes: number
}

export interface NicStat {
  name: string
  /** 累计字节，需前端做差分得到速率 */
  rx_bytes: number
  tx_bytes: number
}

export interface ProcStat {
  pid: number
  user: string
  /** 0 - 100，单核归一后的 CPU 百分比（与 ps pcpu 一致） */
  cpu_percent: number
  /** 0 - 100 */
  mem_percent: number
  /** 仅在 BusyBox /proc 采集路径下提供：CPU 累计时间（秒） */
  cpu_time_secs?: number
  /** 仅在 BusyBox /proc 采集路径下提供：RSS（KB） */
  mem_rss_kb?: number
  command: string
}

export interface SysInfo {
  hostname: string
  os_pretty: string
  kernel: string
  /** 系统架构（uname -m），如 "x86_64" / "aarch64" */
  arch: string
  uptime_secs: number
  /** 0 - 100 */
  cpu_percent: number
  cpu_cores: number
  mem_total_kb: number
  mem_used_kb: number
  swap_total_kb: number
  swap_used_kb: number
  disks: DiskUsage[]
  /** 累计字节（所有网卡合计），需前端做差分得到速率 */
  net_rx_bytes: number
  net_tx_bytes: number
  /** 各网卡明细（不含 lo / docker* / br-* / veth*） */
  nics: NicStat[]
  /** 按 CPU 占用排序的前 5 个进程 */
  top_cpu: ProcStat[]
  /** 按内存占用排序的前 5 个进程 */
  top_mem: ProcStat[]
}

/** 端口转发类型 */
export type ForwardKind = 'local' | 'remote' | 'dynamic'

/** 端口转发规则（与后端 service::forward::ForwardRule 对齐） */
export interface ForwardRule {
  id: string
  sid: string
  kind: ForwardKind
  bindAddr: string
  bindPort: number
  destHost: string | null
  destPort: number | null
  status: string
  err: string | null
  rxBytes: number
  txBytes: number
}

/** 创建端口转发的入参 */
export interface ForwardCreate {
  sid: string
  kind: ForwardKind
  bindAddr: string
  bindPort: number
  destHost?: string | null
  destPort?: number | null
}
