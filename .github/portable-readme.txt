AShell v{{VERSION}} Windows 便携版
====================================

免安装绿色版本：解压到任意目录，双击运行 AShell.exe 即可。

包含文件
--------
- AShell.exe        主程序
- app-ai.exe        AI 助手 sidecar 进程（AI 功能依赖，请勿单独删除）
- OpenConsole.exe   本地终端 ConPTY 宿主（本地终端功能依赖）
- conpty.dll        本地终端 ConPTY 组件（同上）

注意事项
--------
1. 本程序依赖 Microsoft WebView2 运行时（Evergreen）。Windows 11 及
   大部分更新过的 Windows 10 已内置；若启动时提示缺少 WebView2，请到
   https://developer.microsoft.com/microsoft-edge/webview2/
   下载 "Evergreen Standalone Installer" 安装后重新运行。

2. 数据目录为 %USERPROFILE%\.ashell（主机库、连接配置、界面设置等），
   与安装版完全互通；同一台机器上安装版与便携版共存时共享同一份数据。

3. 主机登录凭证使用本机加密密钥保护（Windows 凭据管理器）。把解压目录
   拷贝到其它电脑后，已保存的密码/私钥将无法解密，需重新录入；主机清单
   可用应用内「设置 → 备份/恢复」导出导入（含密码时需输入备份密码）。

问题反馈
--------
https://github.com/vcqr/ashell
