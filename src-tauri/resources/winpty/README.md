# winpty/ — 老系统本地终端的 winpty 回退后端二进制（仅 x86_64）

存放 rprichard/winpty 官方 0.4.3 release 的 x64 组件（MIT 许可，见 LICENSE）：

- `x86_64/winpty.dll` + `x86_64/winpty-agent.exe` — 来自
  https://github.com/rprichard/winpty/releases/tag/0.4.3 的
  `winpty-0.4.3-msvc2015.zip`（x64/bin/ 下两个文件）

## 为什么需要它

系统 ConPTY（kernel32 的 CreatePseudoConsole）自 Windows 10 1809 / Server 2019
起才有。更老的系统（Win10 1809 前 / Server 2016）以及部分 Wine 环境没有
ConPTY，而 portable-pty 0.9 对缺符号会直接 panic（其 win/psuedocon.rs:48，
先于侧载 conpty.dll 分支）。ashell 通过 kernel32 探测（`conpty_supported`）
识别这类系统，本地终端改走 winpty-rs 的 WinPTY 后端（service/local_pty_winpty.rs），
让老系统也能真正使用本地终端。

- winpty 是纯 Win32 实现（agent 进程 + 命名管道），不依赖任何 ConPTY API，
  Win7 起即可用，Wine 下也能正常运行。
- ARM64 不参与：winpty 上游没有 arm64 官方二进制，且现实中不存在无 ConPTY
  的 ARM64 Windows 用户群（WoA 设备全系 1809+），ARM64 维持 ConPTY 探测 +
  置灰行为。

## 分发要求（与 conpty/ 同模式但更严格）

- `winpty.dll` 是 ashell.exe（x86_64 Windows 构建）的**静态导入**：缺它
  进程根本无法启动，因此必须随**所有** x86_64 Windows 分发包一起交付
  （NSIS 安装包经 `tauri.win64.conf.json` 映射到安装根目录、便携包由 CI
  收集、开发构建由 build.rs 复制到 exe 旁）。
- `winpty-agent.exe` 必须与 `winpty.dll` **同目录**：winpty 上游
  AgentLocation 从 dll 自身所在目录查找 agent，不走 PATH。
- build.rs（`CARGO_CFG_TARGET_OS == "windows"` 且 `x86_64` 时）把两个文件
  复制到产物 exe 旁，并暂存固定名到 `resources/winpty/bin/`
  （gitignore）供 tauri.win64.conf.json 资源映射。

## 构建期工具链（CI x86_64 Windows job）

winpty-rs 以编译期链接方式使用 winpty（非 LoadLibrary），构建机需要：

- `winpty-agent.exe` 在 PATH 上（build.rs 探测 + PE 架构校验）；
- `winpty.lib` 可被找到（agent 同目录 / winpty 根目录 / `LIB` 均可，CI 用
  同一份 zip 解压出的 `x64/lib/winpty.lib`，位于 winpty 根目录下）；
- `nuget.exe`（conpty_local feature 拉取 Microsoft.Windows.Console.ConPTY，
  版本由仓库根 `.cargo/config.toml` 的 `WINPTY_RS_CONPTY_VERSION` 钉住，
  与 `resources/conpty` 同一 NuGet 包）。

## 升级方式

从 winpty 新 release 下载 msvc2015 zip，替换 `x86_64/` 下的两个文件，
并同步检查 winpty-rs 版本兼容性（其 build.rs 对 dll 做 PE 架构校验）。
同时更新 `.cargo/config.toml` 里的 ConPTY NuGet 版本与 `resources/conpty/`
（两者必须同版本，保证 conpty.lib 与随包 conpty.dll 导出一致）。
