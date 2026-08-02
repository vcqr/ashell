# AShell

[English](./README.en.md) | 简体中文

> 一个现代化、跨平台的终端 / SSH 客户端 —— 把本地终端、远程 SSH、SFTP、主机监控、端口转发与 AI 助手整合到一个桌面应用里。

AShell 注重性能、隐私与可定制性。所有 SSH 凭证本地加密存储，AI 助手在你自己的机器上运行，密钥不离开你的设备。

![AShell](./ui.png)

---

## 功能特性

### 🖥 终端
- **跨平台**：macOS / Windows / Linux，覆盖 x86_64 与 ARM64。
- **多 Tab 多窗口**：Tab 拖拽重排、右键菜单（重连 / 断开 / 复制连接 / 新窗口 / 复制 SSH 地址 / 重命名 / 导出会话），跨窗口无缝操作。
- **本地终端**：Windows ConPTY / Unix forkpty，支持 PowerShell、cmd、Git Bash、Bash、Zsh、Fish。
- **远程 SSH**：PTY + 二进制安全帧透传，颜色控制码完整保留。
- **Telnet / 串口**：除 SSH 外，支持 Telnet 与串口终端连接。
- **渲染加速**：WebGL 渲染 + 连字（ligatures）+ 网页链接可点 + Unicode 11 + 搜索 + 序列化。
- **进度条识别**：自动识别 `cargo` / `brew` / `wget` / `curl` / `git` / `rsync` / `docker` 等命令的文本进度，同步到任务栏与终端顶部。
- **命令输入建议**：Trie 前缀匹配 + 历史关键字搜索，可清空历史。
- **sudo 自动填充**：识别 sudo 密码提示并自动填充。
- **可配置操作**：Ctrl + 滚轮调字号、鼠标右键 / 中键行为可配、Ctrl + F 搜索浮层。
- **断连重连**：离线恢复显示"重新连接"浮层按钮，本地终端退出后自动关闭 Tab。

### 📁 主机管理
- **无限层级目录树**：拖拽移动子树、级联删除、跨目录复制，按目录组织你的主机。
- **凭证加密**：密码 / 私钥用 AES-256-GCM 加密落盘，永不通过接口外泄。
- **主机元信息**：名称、地址、端口、用户名、图标、颜色、描述，自由归类。
- **可调宽侧面板**：边缘拖拽调宽，宽度持久化记忆。
- **左右分栏表单**：基本信息与连接配置分栏，编辑更清晰。

### 📂 SFTP 文件管理
- **完整 CRUD**：列出（含属主 / 权限 / 时间戳）、新建目录 / 文件、删除、重命名、属性查看。
- **流式传输**：上传带进度与可取消、下载走原生"另存为"对话框、支持文件夹整传。
- **路径面包屑**：可点击 + 可编辑，快速跳转。
- **连接复用**：与终端共享同一 SSH 连接，无需重新认证。
- **在线编辑**：内置 CodeMirror 编辑器，支持查找 / 替换、字号调节、行列状态栏。
- **列宽可调**：表格列宽可拖动调整。
- **权限着色**：权限列按属主 / 属组 / 其他三段独立着色。

### 📊 主机监控
- **实时资源**：CPU / 内存 / 交换 / 磁盘 / 累计网络字节，1.5s 节流轮询。
- **Top 5 进程**：兼容 procps 与 BusyBox。
- **网络流量**：纯 SVG 双线（rx/tx）环形缓冲图，网卡可切换。
- **磁盘折叠**：根挂载主显示，其余挂载点折叠收起。

### 🔌 端口转发
- **三种模式**：本地（`-L`）、远程（`-R`）、动态（`-D`）。
- **可视化规则表**：规则列表 + 流量 / 状态实时刷新。
- **图形化创建**：表单填空即可新增转发规则，无需手敲命令。

### 📡 广播输入
- **多 Tab 同步**：按键级实时转发到多个终端，目标 Tab 可勾选。
- **跨窗口支持**：源 Tab 可选、自动追加回车，跨窗口广播无障碍。

### 🤖 AI 助手
- **多 Agent 驱动**：Claude（sidecar-cc）与 Pi coding agent（sidecar-pi），每个终端会话独立进程。
- **发送给 AI**：终端选中文本与 SFTP 右键菜单支持发送给 AI 助手。
- **执行过程可折叠**：工具调用与返回聚合成可折叠块，对话整洁可读。
- **破坏性操作确认**：执行前弹 y/n 确认条，敏感信息（SSH 会话凭证）不直接透露。
- **远程命令执行**：AI 自动复用当前 SSH 会话，在你授权后执行远程命令。
- **模型可配**：支持自定义 API Key、Base URL、模型名，按 sidecar 类型区分显示，兼容代理与自托管端点。

### 🎨 窗口与外观
- **透明度滑块**：实时调整窗口不透明度。
- **Acrylic 毛玻璃**：macOS / Windows 11 原生模糊效果。
- **背景壁纸**：独立透明度，壁纸与窗口透明分离。
- **系统字体枚举**：自动列出已安装字体，去重 + 字典序。
- **主题**：暗色 / 亮色，配色统一从主题变量派生。

### 🌐 国际化与启动
- **双语**：简体中文 / English 完整覆盖，UI 语言随选随切。
- **会话恢复**：记住上次的 Tab 列表，启动时骨架恢复。
- **自动连接**：启动时自动连接记住的 Tab。
- **默认 Shell**：可选 Auto / PowerShell / cmd / Git Bash / Bash / Zsh / Fish，启动自动打开本地终端。

### 🔄 自动更新
- **启动自动检查**：应用启动后自动检查 GitHub Releases 是否有新版本，发现时弹出通知。
- **一键更新**：通知或「设置 > 关于」中点击「下载并安装」，下载完成后自动重启。
- **更新日志**：检查到新版本时展示 Release Notes（Markdown 渲染）。

---

## 下载安装

前往 [Releases](https://github.com/vcqr/ashell/releases) 下载对应平台的安装包：

| 平台 | 文件 |
|---|---|
| macOS (Apple Silicon) | `*.dmg` |
| macOS (Intel) | `*.dmg` |
| Windows x64 | `*.msi` / `*.exe` |
| Windows ARM64 | `*.msi` / `*.exe` |
| Linux x64 | `*.deb` / `*.rpm` / `*.AppImage` |
| Linux ARM64 | `*.deb` / `*.rpm` / `*.AppImage` |

---

## 从源码构建

### 环境要求

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/)（stable）
- [Bun](https://bun.sh/)（编译 sidecar 二进制）
- 平台依赖：macOS 需 Xcode CLT；Windows 需 MSVC；Linux 需 webkit2gtk 等

### 步骤

```bash
# 1. 安装前端依赖
npm install

# 2. 安装 sidecar 依赖
cd sidecar-cc && npm install && cd ..
cd sidecar-pi && npm install && cd ..

# 3. 开发模式（Vite + Rust 热重载）
npm run tauri dev
#    dev 不会自动编译 sidecar，如需 AI 助手请先手动执行：
npm run sidecar:build

# 4. 生产构建（自动编译 sidecar + 前端 + 打包）
npm run tauri build
```

> `npm run tauri build` 会先自动执行 `npm run sidecar:build`（编译 Claude / Pi sidecar 二进制），再执行 `npm run build`（前端类型检查 + Vite 构建），无需手动编译 sidecar。

---

## 自动更新配置（开发者）

AShell 集成了 Tauri 2 Updater，通过 GitHub Releases 分发更新。首次启用需要完成以下一次性配置：

### 1. 生成签名密钥

```bash
npx @tauri-apps/cli signer generate -w ~/.tauri/ashell.key
```

按提示设置密码，命令会输出 Public Key 并将私钥写入指定文件。

### 2. 配置公钥

将上一步输出的 Public Key 填入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey` 字段（替换占位符 `REPLACE_WITH_YOUR_PUBLIC_KEY`）。

### 3. 添加 GitHub Secrets

在仓库 **Settings → Secrets and variables → Actions** 中添加：

| Secret 名 | 值 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥文件完整内容 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 生成密钥时设置的密码 |

配置完成后，每次推送 `v*` 标签触发 `tauri-action` 时会自动签名更新包并生成 `latest.json` 上传到 Release，应用内的检查更新即可正常工作。

---

## 许可证

MIT
