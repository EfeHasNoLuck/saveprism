; Explorer "Decode with SavePrism" entry for every file.
;
; Tauri's NSIS template sets SHCTX from the installer's install mode (HKCU for a per-user
; install, HKLM for a per-machine one) and calls these hooks inside the install/uninstall
; sections, so the entry lives in the same hive as the app.
;
; Windows 11 shows this classic verb under "Show more options"; the modern top-level menu is not
; needed. The label uses the installer's language: Turkish when the selected NSIS language is
; Turkish (1055), English for every other language.

!macro SavePrismWriteDecodeEntry
  ${If} $LANGUAGE == 1055
    ; "SavePrism ile çöz", spelled with code points so the file's encoding cannot garble it.
    StrCpy $R9 "SavePrism ile ${U+00E7}${U+00F6}z"
  ${Else}
    StrCpy $R9 "Decode with SavePrism"
  ${EndIf}
  WriteRegStr SHCTX "Software\Classes\*\shell\SavePrism" "" $R9
  WriteRegStr SHCTX "Software\Classes\*\shell\SavePrism" "MUIVerb" $R9
  WriteRegStr SHCTX "Software\Classes\*\shell\SavePrism" "Icon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr SHCTX "Software\Classes\*\shell\SavePrism\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro SavePrismWriteDecodeEntry
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey SHCTX "Software\Classes\*\shell\SavePrism"
!macroend
