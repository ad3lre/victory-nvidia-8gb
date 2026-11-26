; Ad3lHub NSIS hooks — Start Menu is created by Tauri; add Desktop shortcut too.
!macro NSIS_HOOK_POSTINSTALL
  CreateShortCut "$DESKTOP\Ad3lHub.lnk" "$INSTDIR\Ad3lHub.exe" "" "$INSTDIR\Ad3lHub.exe" 0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$DESKTOP\Ad3lHub.lnk"
!macroend
