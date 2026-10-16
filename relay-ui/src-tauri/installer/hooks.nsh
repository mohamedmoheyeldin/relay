; Relay NSIS hooks (Tauri bundler).

; The Cloud AI CLI's executable name, as relay-core/src/cli_names.rs has it (D-110).
!define CLOUD_CLI "cloud"
; Relay installs for all users into Program Files. Relay 0.1.0 installed per user
; (C:\relay by default), so that copy is uninstalled first: there is only ever one Relay.

; Relay 0.1.9 asked installed Chromium browsers to add the Playwright Extension through
; the ExtensionInstallForcelist policy. The local operator now drives browsers through
; UI Automation with no extension (D-089), so installing or updating takes that entry
; out again, as does uninstalling. Only the entry Relay added is removed (a marker
; under HKLM\Software\Relay records where), and the numbered list stays without gaps
; by moving the last entry into the freed slot.
!define RELAY_EXT_ID "mmlmfjhmonkocbjadbfplnigmagldckm"

; $R9 = policy key under HKLM, $R8 = marker name.
!macro RELAY_REMOVE_EXT_POLICY_BODY
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
  DeleteRegKey /ifempty HKLM "Software\Relay"
  relay_rm_done:
  SetRegView lastused
!macroend

Function RelayRemoveExtPolicy
  !insertmacro RELAY_REMOVE_EXT_POLICY_BODY
FunctionEnd

Function un.RelayRemoveExtPolicy
  !insertmacro RELAY_REMOVE_EXT_POLICY_BODY
FunctionEnd

!macro RELAY_EXT_POLICIES_REMOVE UN
  StrCpy $R9 "SOFTWARE\Policies\BraveSoftware\Brave"
  StrCpy $R8 "ExtPolicyBrave"
  Call ${UN}RelayRemoveExtPolicy
  StrCpy $R9 "SOFTWARE\Policies\Google\Chrome"
  StrCpy $R8 "ExtPolicyChrome"
  Call ${UN}RelayRemoveExtPolicy
  StrCpy $R9 "SOFTWARE\Policies\Microsoft\Edge"
  StrCpy $R8 "ExtPolicyEdge"
  Call ${UN}RelayRemoveExtPolicy
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
  !insertmacro RELAY_EXT_POLICIES_REMOVE ""
!macroend

; The app registers relay-screen and relay-browser with Cloud AI (D-047), so the
; uninstaller takes those registrations out again. Cloud AI itself, the models and
; settings are left alone.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$SYSDIR\cmd.exe" /c ${CLOUD_CLI} mcp remove --scope user relay-screen'
  Pop $0
  nsExec::Exec '"$SYSDIR\cmd.exe" /c ${CLOUD_CLI} mcp remove --scope user relay-browser'
  Pop $0
  !insertmacro RELAY_EXT_POLICIES_REMOVE "un."
!macroend

; Python writes __pycache__ files next to the bundled relay-screen as it runs; the
; uninstaller only knows the files it installed, so remove the whole folder.
!macro NSIS_HOOK_POSTUNINSTALL
  RMDir /r "$INSTDIR\python"
  RMDir "$INSTDIR"
!macroend
