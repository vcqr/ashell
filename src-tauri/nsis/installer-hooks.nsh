; Tauri NSIS 自定义 hook（tauri.windows.conf.json 的 bundle.windows.nsis.installerHooks 引入）。
; 安装后注册 / 卸载后删除资源管理器右键菜单「在 Ashell 中打开 / Open in Ashell」：
;   Directory\shell           —— 右键文件夹
;   Directory\Background\shell —— 文件夹内空白处右键
;   Drive\shell               —— 右键驱动器
; HKCU + per-user 安装，无需管理员权限；updater 静默重装会重复执行，写键幂等。
; 应用内开关（context_menu.rs）读写同一组键，两端保持一致。
;
; 注意：菜单文本不要用 LangString——实测（2026-11）在 Tauri NSIS hook 里
; $(LangString) 解析为空串，Explorer 只好显示键名 AshellOpen。这里改用
; $LANGUAGE 运行时比较（安装语言；passive 静默升级时由 Tauri 按上次安装
; 语言继承），拼接的全是静态字符串，不依赖语言表。

!include "LogicLib.nsh"

Var _ashellMenuText

!macro _ashellWriteContextMenu
  WriteRegStr HKCU "Software\Classes\Directory\shell\AshellOpen" "" "$_ashellMenuText"
  WriteRegStr HKCU "Software\Classes\Directory\shell\AshellOpen" "Icon" "$INSTDIR\ashell.exe"
  WriteRegStr HKCU "Software\Classes\Directory\shell\AshellOpen\command" "" '"$INSTDIR\ashell.exe" "%V"'

  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\AshellOpen" "" "$_ashellMenuText"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\AshellOpen" "Icon" "$INSTDIR\ashell.exe"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\AshellOpen\command" "" '"$INSTDIR\ashell.exe" "%V"'

  WriteRegStr HKCU "Software\Classes\Drive\shell\AshellOpen" "" "$_ashellMenuText"
  WriteRegStr HKCU "Software\Classes\Drive\shell\AshellOpen" "Icon" "$INSTDIR\ashell.exe"
  WriteRegStr HKCU "Software\Classes\Drive\shell\AshellOpen\command" "" '"$INSTDIR\ashell.exe" "%V"'
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LANGUAGE == ${LANG_SIMPCHINESE}
    StrCpy $_ashellMenuText "在 Ashell 中打开"
  ${Else}
    StrCpy $_ashellMenuText "Open in Ashell"
  ${EndIf}
  !insertmacro _ashellWriteContextMenu
!macroend

; DeleteRegKey 默认连同子键一起删，command 子键随之清除
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Classes\Directory\shell\AshellOpen"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\AshellOpen"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\AshellOpen"
!macroend
