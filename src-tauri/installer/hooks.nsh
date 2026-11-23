; Extra steps for Tauri's NSIS installer.

; Uninstalling also takes Cerberus out of what Windows opens at sign-in
; ("Abrir com o Windows" in Configurações), so nothing points at a missing program.
; Updates run the uninstaller too; they keep it, since the new version is the same program.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Cerberus Launcher"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Cerberus Launcher"
    ; Where the installer kept the folder and language for a reinstall.
    DeleteRegKey HKCU "Software\Cerberus Launcher\Cerberus Launcher"
    DeleteRegKey /ifempty HKCU "Software\Cerberus Launcher"
  ${EndIf}
!macroend
