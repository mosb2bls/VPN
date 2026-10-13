; NSIS installer hooks (Tauri `bundle.windows.nsis.installerHooks`; ADR-0007).
;
; NSIS_HOOK_PREUNINSTALL runs before the uninstaller closes the app and
; deletes files. It:
;   1. restores networking through the app's own CLI (system proxy / TUN
;      session), so an uninstall never leaves the proxy pointing at a dead
;      local port;
;   2. removes the TUN helper service if it is installed. The service and its
;      files live outside $INSTDIR (Program Files / ProgramData) and would
;      otherwise keep starting at boot. This needs admin rights → one UAC
;      prompt; if declined, the service stays (TESTING.md W-INSTALL).

!macro NSIS_HOOK_PREUNINSTALL
  Push $0
  IfFileExists "$INSTDIR\${MAINBINARYNAME}.exe" 0 edge_skip_restore
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --restore-networking'
  edge_skip_restore:
  ClearErrors
  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\io.github.mosb2bls.edgeclient.helper" "ImagePath"
  StrCmp $0 "" edge_skip_helper 0
  IfFileExists "$INSTDIR\edge-helper.exe" 0 edge_skip_helper
    ExecShellWait "runas" "$INSTDIR\edge-helper.exe" "uninstall" SW_HIDE
  edge_skip_helper:
  Pop $0
!macroend

; Task Manager's "startup approved" record for our login item (HKCU) is not
; removed with the Run value; drop it with the app (round 1, M-2).
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
!macroend
