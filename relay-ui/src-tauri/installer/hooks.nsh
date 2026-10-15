; Relay NSIS hooks (Tauri bundler).
; Relay installs for all users into Program Files. Relay 0.1.0 installed per user
; (C:\relay by default), so that copy is uninstalled first: there is only ever one Relay.

!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Relay" "InstallLocation"
  StrCmp $R0 "" relay_old_done
  ; InstallLocation is stored with quotes ("C:\relay").
  StrCpy $R1 $R0 1
  StrCmp $R1 '"' 0 +3
  StrCpy $R0 $R0 "" 1
  StrCpy $R0 $R0 -1
  StrCmp $R0 $INSTDIR relay_old_keys
  IfFileExists "$R0\uninstall.exe" 0 relay_old_keys
  DetailPrint "Removing the earlier Relay in $R0"
  ; _?= runs the old uninstaller in place, so ExecWait really waits for it.
  ExecWait '"$R0\uninstall.exe" /S _?=$R0'
  Delete "$R0\uninstall.exe"
  RMDir /r "$R0\python"
  RMDir "$R0"
  relay_old_keys:
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Relay"
  relay_old_done:
  ; The per-user install's remembered folder; per-machine installs keep theirs in HKLM.
  DeleteRegKey HKCU "Software\Relay"
!macroend

; The app registers relay-screen and relay-browser with Claude Code (D-047), so the
; uninstaller takes those registrations out again. Claude Code itself, the models and
; settings are left alone.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$SYSDIR\cmd.exe" /c claude mcp remove --scope user relay-screen'
  Pop $0
  nsExec::Exec '"$SYSDIR\cmd.exe" /c claude mcp remove --scope user relay-browser'
  Pop $0
!macroend

; Python writes __pycache__ files next to the bundled relay-screen as it runs; the
; uninstaller only knows the files it installed, so remove the whole folder.
!macro NSIS_HOOK_POSTUNINSTALL
  RMDir /r "$INSTDIR\python"
  RMDir "$INSTDIR"
!macroend
