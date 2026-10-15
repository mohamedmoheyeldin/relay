; Relay NSIS hooks (Tauri bundler).
; Relay installs for all users into Program Files. Relay 0.1.0 installed per user
; (C:\relay by default), so that copy is uninstalled first: there is only ever one Relay.

; The browser lane drives the owner's open browser through the Playwright Extension.
; The owner asked (2026-10-15) for Relay to add it with no clicks, accepting that the
; browser then shows "Managed by your organization". The installer runs elevated, so
; it sets the ExtensionInstallForcelist policy for each installed Chromium browser,
; and browsers apply it while running. List policies are read as values "1", "2", ...
; with no gaps, so Relay's entry takes the first free slot and uninstall moves the
; last entry into its slot. A marker under HKLM\Software\Relay records where Relay
; added it, so an entry the owner set themselves is never removed.
!define RELAY_EXT_ID "mmlmfjhmonkocbjadbfplnigmagldckm"
!define RELAY_EXT_ENTRY "mmlmfjhmonkocbjadbfplnigmagldckm;https://clients2.google.com/service/update2/crx"

; $R9 = policy key under HKLM, $R8 = marker name.
Function RelayAddExtPolicy
  SetRegView 64
  StrCpy $R1 1
  relay_add_loop:
    ClearErrors
    ReadRegStr $R2 HKLM "$R9\ExtensionInstallForcelist" "$R1"
    IfErrors relay_add_new
    StrCpy $R3 $R2 32
    StrCmp $R3 "${RELAY_EXT_ID}" relay_add_done
    IntOp $R1 $R1 + 1
    Goto relay_add_loop
  relay_add_new:
    WriteRegStr HKLM "$R9\ExtensionInstallForcelist" "$R1" "${RELAY_EXT_ENTRY}"
    WriteRegDWORD HKLM "Software\Relay" "$R8" 1
    DetailPrint "Added the Playwright Extension policy ($R8)"
  relay_add_done:
  SetRegView lastused
FunctionEnd

Function un.RelayRemoveExtPolicy
  SetRegView 64
  ClearErrors
  ReadRegDWORD $R0 HKLM "Software\Relay" "$R8"
  IfErrors relay_rm_done
  StrCpy $R1 1
  StrCpy $R4 0
  StrCpy $R5 0
  relay_rm_loop:
    ClearErrors
    ReadRegStr $R2 HKLM "$R9\ExtensionInstallForcelist" "$R1"
    IfErrors relay_rm_end
    StrCpy $R3 $R2 32
    StrCmp $R3 "${RELAY_EXT_ID}" 0 +2
    StrCpy $R4 $R1
    StrCpy $R5 $R1
    IntOp $R1 $R1 + 1
    Goto relay_rm_loop
  relay_rm_end:
  ; $R4 = Relay's slot (0 when gone), $R5 = the last slot.
  StrCmp $R4 0 relay_rm_marker
  StrCmp $R4 $R5 relay_rm_last
  ReadRegStr $R2 HKLM "$R9\ExtensionInstallForcelist" "$R5"
  WriteRegStr HKLM "$R9\ExtensionInstallForcelist" "$R4" "$R2"
  relay_rm_last:
  DeleteRegValue HKLM "$R9\ExtensionInstallForcelist" "$R5"
  DeleteRegKey /ifempty HKLM "$R9\ExtensionInstallForcelist"
  DeleteRegKey /ifempty HKLM "$R9"
  relay_rm_marker:
  DeleteRegValue HKLM "Software\Relay" "$R8"
  relay_rm_done:
  SetRegView lastused
FunctionEnd

!macro RELAY_EXT_POLICIES_ADD
  ; Every Brave channel, Brave Origin included, reads BraveSoftware\Brave.
  IfFileExists "$PROGRAMFILES64\BraveSoftware\*.*" 0 relay_ext_chrome
  StrCpy $R9 "SOFTWARE\Policies\BraveSoftware\Brave"
  StrCpy $R8 "ExtPolicyBrave"
  Call RelayAddExtPolicy
  relay_ext_chrome:
  IfFileExists "$PROGRAMFILES64\Google\Chrome\Application\chrome.exe" relay_ext_chrome_add
  IfFileExists "$PROGRAMFILES32\Google\Chrome\Application\chrome.exe" relay_ext_chrome_add relay_ext_edge
  relay_ext_chrome_add:
  StrCpy $R9 "SOFTWARE\Policies\Google\Chrome"
  StrCpy $R8 "ExtPolicyChrome"
  Call RelayAddExtPolicy
  relay_ext_edge:
  IfFileExists "$PROGRAMFILES32\Microsoft\Edge\Application\msedge.exe" 0 relay_ext_done
  StrCpy $R9 "SOFTWARE\Policies\Microsoft\Edge"
  StrCpy $R8 "ExtPolicyEdge"
  Call RelayAddExtPolicy
  relay_ext_done:
!macroend

!macro RELAY_EXT_POLICIES_REMOVE
  StrCpy $R9 "SOFTWARE\Policies\BraveSoftware\Brave"
  StrCpy $R8 "ExtPolicyBrave"
  Call un.RelayRemoveExtPolicy
  StrCpy $R9 "SOFTWARE\Policies\Google\Chrome"
  StrCpy $R8 "ExtPolicyChrome"
  Call un.RelayRemoveExtPolicy
  StrCpy $R9 "SOFTWARE\Policies\Microsoft\Edge"
  StrCpy $R8 "ExtPolicyEdge"
  Call un.RelayRemoveExtPolicy
!macroend

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

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro RELAY_EXT_POLICIES_ADD
!macroend

; The app registers relay-screen and relay-browser with Claude Code (D-047), so the
; uninstaller takes those registrations out again. Claude Code itself, the models and
; settings are left alone.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$SYSDIR\cmd.exe" /c claude mcp remove --scope user relay-screen'
  Pop $0
  nsExec::Exec '"$SYSDIR\cmd.exe" /c claude mcp remove --scope user relay-browser'
  Pop $0
  !insertmacro RELAY_EXT_POLICIES_REMOVE
!macroend

; Python writes __pycache__ files next to the bundled relay-screen as it runs; the
; uninstaller only knows the files it installed, so remove the whole folder.
!macro NSIS_HOOK_POSTUNINSTALL
  RMDir /r "$INSTDIR\python"
  RMDir "$INSTDIR"
!macroend
