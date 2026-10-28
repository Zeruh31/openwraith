; OpenWraith's hooks into Tauri's NSIS installer (`bundle.windows.nsis.installerHooks`).
;
; The uninstaller removes "Start with Windows": the Run value `OpenWraith` that
; the app writes (`app/src-tauri/src/autostart.rs`, `VALUE_NAME`). Not when the
; uninstaller runs for an update (`/UPDATE`): the setting stays, and the new
; version keeps its entry. (Tauri's own uninstaller also deletes a Run value
; named after the product, which is the same name today; this hook does not
; rely on that.)
;
; Task Manager's `StartupApproved\Run` value of the same name is the user's
; choice, which the app only reads. It goes only with the settings, when
; "Delete the application data" is ticked (`$DeleteAppDataCheckboxState`, the
; condition of Tauri's own deletion of the app data): then nothing writes the
; Run value again. Otherwise it stays: a newer setup program uninstalls the
; old version first by default, without `/UPDATE` and with the settings kept,
; and the new version writes the Run value again at its first start, so an
; entry the user disabled in Task Manager stays disabled. (Without a Run value
; the kept value starts nothing, and Task Manager does not list it.)
;
; HKCU is the right hive because the installer is per user (`installMode`
; `currentUser`, Tauri's default: no administrator rights, so the uninstaller
; runs as the user who installed it). A per-machine installer would run the
; uninstaller elevated, possibly as another account, so the build refuses
; any other mode until this hook is changed with it.

!macro NSIS_HOOK_POSTUNINSTALL
  !if "${INSTALLMODE}" != "currentUser"
    !error "windows/hooks.nsh removes the start entry from HKCU and needs installMode currentUser"
  !endif
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "OpenWraith"
    ${If} $DeleteAppDataCheckboxState = 1
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "OpenWraith"
    ${EndIf}
  ${EndIf}
!macroend
