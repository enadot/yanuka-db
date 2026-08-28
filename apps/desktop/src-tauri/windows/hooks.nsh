; Migration from the old product name.
;
; Until 0.5.1 the product was named "Contacts", and Tauri's NSIS template keys
; the uninstall registry entry by product name — so the "אוצר שלמה" installer
; would not see that install and Windows would list both apps. This hook runs
; the old entry's own uninstaller silently before installing. The database,
; the encryption key and the backups all live under the identifier
; (digital.baram.yanuka), which the rename does not touch and the uninstaller
; does not delete.
;
; The registry values are written quoted by the same template, hence the
; quote-stripping. `_?=` makes the uninstaller run in place so ExecWait
; actually waits — which also means it cannot delete itself; the Delete/RMDir
; after it finish the job.
!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $R9 SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Contacts" "UninstallString"
  StrCmp $R9 "" contacts_migration_done
  ReadRegStr $R8 SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Contacts" "InstallLocation"
  StrCmp $R8 "" contacts_migration_done
  StrCpy $R7 $R8 1
  StrCmp $R7 '"' 0 +3
  StrCpy $R8 $R8 "" 1
  StrCpy $R8 $R8 -1
  ExecWait '$R9 /S _?=$R8'
  Delete "$R8\uninstall.exe"
  RMDir "$R8"
  DeleteRegKey SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Contacts"
contacts_migration_done:
!macroend
