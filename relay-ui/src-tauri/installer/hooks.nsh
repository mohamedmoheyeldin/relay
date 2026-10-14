; Relay NSIS hooks (Tauri bundler).
; The app registers relay-screen with Claude Code on first start (D-047), so the
; uninstaller takes that registration out again. Claude Code itself, the models and
; settings are left alone.

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$SYSDIR\cmd.exe" /c claude mcp remove --scope user relay-screen'
  Pop $0
!macroend
