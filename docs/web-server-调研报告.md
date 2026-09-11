# AShell Web Server 版本可行性调研报告

> 调研日期：2026-09-11 ｜ 基线版本：0.1.25 ｜ 调研范围：src-tauri / src 全量代码扫描

## 1. 结论（TL;DR）

**可行，且改造成本显著低于一般 Tauri 项目。** 项目当前已是「Tauri 壳 + 内嵌 axum HTTP/WebSocket 服务器」的混合架构：全部核心业务（SSH/SFTP/本地终端/Telnet/串口/主机管理/备份等）本就通过 axum REST/WS 暴露，Tauri IPC 只承担辅助功能。Rust 核心业务层对 Tauri 的依赖总计仅 2 处（且是独立 crate，不依赖 Tauri 核心）。

建议采用**同仓库双目标**方案（`cargo build --bin ashell` 桌面版 / `--bin ashell-server` Web 版），而非复制独立仓库。P0 最小可用版工作量约 2~4 天。

## 2. 现状架构

```
┌─ Tauri WebView (Vue 3 SPA) ─────────────────────────────┐
│  ① 业务请求（99%）                                       │
│     invoke("get_api_info") 拿 base_url + token          │
│     → HTTP/WS → axum (127.0.0.1:0 随机端口) → handlers  │
│  ② 辅助功能（少量）                                      │
│     invoke() → Tauri commands（托盘/热键/对话框/字体/    │
│     壁纸/AI sidecar/窗口管理）                           │
└─────────────────────────────────────────────────────────┘
```

### 2.1 内嵌 API 服务器已具备的能力

入口 `src-tauri/src/lib.rs` 的 `start_api_server()`：初始化配置 → 打开 SQLite（sqlx）→ 绑定 `127.0.0.1:0` → `routers::build_router()` → `axum::serve` 后台运行。前端通过 `get_api_info` 命令取得 `ApiInfo { addr, token, base_url, ws_url }`。

`src-tauri/src/routers/mod.rs` 已挂载的路由（全部核心功能）：

| 分类 | 路由 | 形态 |
|---|---|---|
| 分组/主机 CRUD | `/api/groups` `/api/hosts`（含 ssh-config 导入、凭证 reveal） | REST |
| SSH 终端 | `/api/ssh/terminal/{host_id}` | WebSocket |
| 本地 PTY 终端 | `/api/local/terminal` | WebSocket |
| Telnet / 串口终端 | `/api/telnet/terminal/{id}` `/api/serial/terminal/{id}` | WebSocket |
| SFTP 全套 | open/list/mkdir/touch/remove/rename/move/duplicate/chmod/du/download/upload/close/elevate | REST |
| 本地文件管理 | list/roots/mkdir/rename/copy/move/trash/remove/open/reveal/进度 | REST |
| 端口转发（-L/-R/-D） | `/api/ssh/forward` | REST |
| AI 供应商/引擎/常用语 | `/api/ai-providers` `/api/ai-engines` `/api/ai-phrases` | REST |
| 命令模板 / 操作密码 / known-hosts / 图标 / 备份恢复 | 各自 REST | REST |
| 健康检查 | `/health`（免鉴权） | REST |

基建已就绪：`middleware/auth.rs` Bearer token 鉴权（WS 走 query token）、`middleware/cors.rs`、TraceLayer 请求日志、100GB 流式上传体积限制。

### 2.2 Rust 侧 Tauri 耦合面（`tauri::` 共 54 处 / 8 个文件）

| 文件 | 用途 | Web 版处理 |
|---|---|---|
| `lib.rs` | 应用入口、托盘/热键插件注册、窗口事件 | 不进 server bin |
| `tray.rs` / `hotkey.rs` | 托盘、全局热键 | Web 版无意义，跳过 |
| `commands/dialog.rs` | 原生文件对话框（4 个命令） | 浏览器原生替代 |
| `commands/fonts.rs` | 系统字体列表（font-kit，纯 Rust） | 升级为 HTTP 路由（约 5 行） |
| `commands/wallpaper.rs` | 壁纸存取（std::fs 实现） | 升级为 HTTP 路由 + 静态文件服务 |
| `ai_env.rs` | AI 路径读写/探测 | 升级为 HTTP 路由 |
| `sidecar.rs` | AI 子进程管理 + event 回推 | 需 WS 化改造（P1）或一期裁掉 |

**核心业务层（handlers/service/models/routers/middleware/config）的 Tauri 依赖总计 2 处**：`service/local_fs.rs:483/494` 使用 `tauri_plugin_opener` 打开/显示文件——该 crate 独立于 Tauri 核心，Web 版可直接保留。其余依赖均为纯 Rust crate（axum/tokio/russh/sqlx/font-kit 等）。

### 2.3 前端 Tauri 耦合面（27 个文件引用 `@tauri-apps/*`）

唯一的架构性耦合在 `src/api/client.ts:40`——`invoke("get_api_info")`；其余分类如下：

| 类别 | 涉及文件数 | Web 版替代 |
|---|---|---|
| `invoke` 辅助命令 | 18 | HTTP 路由 / 隐藏 / 浏览器原生 |
| 剪贴板插件 writeText | 3 | `navigator.clipboard` |
| `openUrl`（plugin-opener） | 3 | `window.open` |
| 跨窗口 `emit/listen`（广播输入、AI 事件） | 2 | `BroadcastChannel` 或后端 WS |
| 窗口控制 / 任务栏进度（`getCurrentWindow`） | 3 | Web 版条件渲染隐藏 |
| `WebviewWindow` 独立窗口（SFTP/AI） | 1 | `window.open` |
| 文件对话框（选私钥/选图/存文件/开文件） | 4 处调用 | `<input type="file">` / `a[download]` |
| 自动更新 + relaunch | 1 | Web 版隐藏 |
| `getVersion` | 1 | 后端 `/health` 附带版本或构建注入 |
| `convertFileSrc`（壁纸预览） | 1 | 后端静态文件路由 |

## 3. 推荐方案：同仓库双目标，而非 fork 独立仓库

核心业务层 99% 复用，复制仓库会导致每个新功能写两遍。推荐在同一 Cargo 包内增加第二个二进制目标：

```
cargo build --bin ashell          # 桌面版（现状不变）
cargo build --bin ashell-server   # Web 服务器版（新增）
```

- `tauri` 及其插件依赖放入 default feature，server bin 不启用；`commands/`、`tray.rs`、`hotkey.rs`、`sidecar.rs` 等目录 feature-gate。
- 前端同一套代码，新增 `src/utils/platform.ts` 适配层：`isTauri = '__TAURI_INTERNALS__' in window`，把 27 处 `@tauri-apps/*` 直接 import 收敛到适配层，运行时按形态分派。
- 静态资源：`tower-http` 已启用 `features = ["full"]`（含 `ServeDir`），直接托管 `dist/` 即可，无需新增依赖。

## 4. 实施拆解与工作量估算

### P0 —— 最小可用 Web 版（约 2~4 天）

SSH/SFTP/本地终端/Telnet/串口/主机管理全部可用：

1. **Rust**：新增 `src/bin/ashell-server.rs`（约 150 行）：config init → DB pool → `build_router()` → 绑定固定端口 → `axum::serve` + `ServeDir(dist)`，启动时打印访问 URL 与 token。
2. **Rust**：`list_system_fonts`、wallpaper、ai_env 升级为 HTTP 路由（各约 5~10 行）。
3. **前端**：`client.ts` 的 `get_api_info` 改为同源部署（`base_url = ''`）+ 登录页输入 token 存 localStorage；适配层替换剪贴板/openUrl/对话框/版本号；窗口控制、托盘、热键、更新器等 UI 条件隐藏。

### P1 —— 完整体验（约 +2 天）

- AI sidecar：spawn/write/kill 改为 HTTP，stdout 回推改 axum WS 广播（Rust 中等工作量）。
- 壁纸预览静态路由、跨窗口广播改 `BroadcastChannel`、AI/设置独立窗口改 `window.open`。

### P2 —— 部署形态（视需求）

HTTPS 反代文档、Docker 镜像、只读演示模式等。

## 5. 风险与待决策项

| # | 事项 | 说明 |
|---|---|---|
| 1 | **监听地址与鉴权模型**（需决策） | 现状为单用户模型：单 Bearer token 明文存 `~/.ashell/config`、SQLite 单用户、WS query token。绑 `127.0.0.1` 自用没有问题；若绑 `0.0.0.0` 对外，需登录页输 token + HTTPS 反代；**多人共用会存在会话与配置交叉**，如需多租户属更大的改造 |
| 2 | **AI 功能一期取舍**（需决策） | sidecar WS 化是唯一较大的单项改造，建议一期 Web 版先隐藏 AI 助手 |
| 3 | 本地文件对话框 | 浏览器 `input[type=file]` 无法获得任意绝对路径，选私钥等场景需改为上传文件内容而非传路径 |
| 4 | 全局热键/托盘/任务栏进度 | 浏览器平台无对应能力，Web 版 UI 需隐藏相关设置项，文案与设置页需同步调整 |

## 6. 关键文件索引

- Rust 入口与 API 启动：`src-tauri/src/lib.rs`
- 路由全表：`src-tauri/src/routers/mod.rs`
- 鉴权/CORS 中间件：`src-tauri/src/middleware/`
- 前端 API 客户端（唯一 IPC 耦合点）：`src/api/client.ts`
- Tauri commands（需 HTTP 化/替代）：`src-tauri/src/commands/`、`ai_env.rs`、`sidecar.rs`
- 前端平台耦合清单：`src/utils/newWindow.ts`、`src/stores/broadcast.ts`、`src/composables/useWindowControls.ts` 等（详见 §2.3）
