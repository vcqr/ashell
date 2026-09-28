# conpty/ — Windows ConPTY 官方侧载二进制

本目录存放 microsoft/terminal 官方发布的 ConPTY 组件（MIT 许可，见 LICENSE）：

- `conpty.dll` — ConPTY API 侧载层（portable-pty 会优先于 kernel32 加载它，见
  portable-pty 0.9.0 `src/win/psuedocon.rs::load_conpty`）
- `OpenConsole.exe` — conhost 修复版（conpty.dll 从自身所在目录拉起它）

来源：NuGet 包 `Microsoft.Windows.Console.ConPTY.1.24.260710001`
（https://github.com/microsoft/terminal/releases/tag/v1.24.11911.0 ，x64）。

## 为什么需要它

系统 conhost（10.0.26100.8328 实测）在解析 opencode 等基于 opentui 的 TUI 退出时
的输出流会崩溃（Application 事件日志 1000，conhost.exe 0xc0000005，崩溃点位于
备用屏幕退出后的重绘路径），表现为整个本地终端会话随宿主 shell 一起死亡。
Windows Terminal 自带的新版 OpenConsole 无此问题；随包侧载同一代 OpenConsole +
conpty.dll 后 ashell 本地终端同样恢复正常（opencode `/exit` 后会话信息正常打印、
shell 存活）。

升级方式：从 terminal 新 release 下载同版本 ConPTY NuGet，替换本目录两个文件
（x64）。构建时 build.rs 会把这两个文件复制到开发 exe 旁；打包经
tauri.windows.conf.json 的 resources 映射放到安装目录根（与 ashell.exe 同目录）。
