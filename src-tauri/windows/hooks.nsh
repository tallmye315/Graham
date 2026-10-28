!macro NSIS_HOOK_POSTINSTALL
  ; Retire only the removed securities-logo subsystem. Financial data stays in place.
  Delete "$INSTDIR\market\logos.py"
  Delete "$INSTDIR\market\logo-sources.json"
  Delete "$INSTDIR\market\__pycache__\logos.*.pyc"
  RMDir "$INSTDIR\market\__pycache__"
  RMDir /r "$LOCALAPPDATA\com.graham.portfolio\company-logos"
  Push $0
  ClearErrors
  ExecWait '"$INSTDIR\graham.exe" --repair-shortcuts' $0
  ${If} ${Errors}
    Pop $0
    Abort "无法创建 Graham 快捷方式，请检查桌面和开始菜单目录权限。"
  ${ElseIf} $0 != 0
    Pop $0
    Abort "无法创建 Graham 快捷方式，请检查桌面和开始菜单目录权限。"
  ${EndIf}
  Pop $0
!macroend
