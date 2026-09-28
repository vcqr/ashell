# conpty/ — Windows ConPTY 官方侧载二进制（按架构分目录）

存放 microsoft/terminal 官方发布的 ConPTY 组件（MIT 许可，见 LICENSE）：

- `x86_64/` — NuGet 的 x64 变体（`conpty.dll` + `OpenConsole.exe`）
- `aarch64/` — NuGet 的 arm64 变体
- `bin/` — 构建暂存（gitignore）：build.rs 按目标架构把所选变体以固定名复制到这里，
  供 tauri.windows.conf.json 的 resources 映射进安装包

来源：NuGet 包 `Microsoft.Windows.Console.ConPTY.1.24.260710001`
（https://github.com/microsoft/terminal/releases/tag/v1.24.11911.0 ）。

## 工作机制

- portable-pty 0.9.0 创建伪控制台时，优先加载 **exe 同目录**的 `conpty.dll`
  （LoadLibrary 默认先搜应用目录），找不到才回退 kernel32 的系统实现；
  `conpty.dll` 再从自身所在目录拉起 `OpenConsole.exe`。
- `build.rs`（`CARGO_CFG_TARGET_OS == "windows"` 时）按 `CARGO_CFG_TARGET_ARCH`
  选择变体目录，复制到产物 exe 旁（兼容有无 `--target` 的布局），并暂存固定名
  文件供打包映射；macOS/Linux 目标不执行任何复制。
- 构建命令无需任何改动：CI 里 `tauri build --target x86_64-pc-windows-msvc`
  与 `--target aarch64-pc-windows-msvc` 会各自拿到对应架构的变体。

## 为什么需要它

系统 conhost（10.0.26100.8328 实测）在解析 opencode 等基于 opentui 的 TUI 退出时
的输出流会崩溃（Application 事件日志 1000，conhost.exe 0xc0000005，崩溃点位于
备用屏幕退出后的重绘路径），表现为整个本地终端会话随宿主 shell 一起死亡。
Windows Terminal 自带的新版 OpenConsole 无此问题；随包侧载同一代 OpenConsole +
conpty.dll 后 ashell 本地终端同样恢复正常（opencode `/exit` 后会话信息正常打印、
shell 存活）。

## 升级方式

从 terminal 新 release 下载对应版本 ConPTY NuGet，替换 `x86_64/` 与 `aarch64/`
下的两个文件。如需支持 32 位 Windows，再补 `i686/`（NuGet 的 x86 变体）并在
build.rs 的架构映射中加入。
