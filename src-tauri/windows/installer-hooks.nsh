; Tauri includes this file near the top of its installer.nsi, before the template's own
; defines, variables and pages. Nothing here may depend on them.

!ifdef MUI_INTERFACE
  !error "installer-hooks.nsh must be included before the first Modern UI page"
!endif

!define /ifndef ERROR_MORE_DATA 234
!define POROS_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\$(^Name)"

!define MUI_CUSTOMFUNCTION_GUIINIT OfferUpgrade

Var InstalledVersion
Var BundledVersion
Var UpgradePrompt

; When an older version is installed, ask once and then rerun this installer in Tauri's
; updater mode (/P /UPDATE /R): progress window only, same folder and shortcuts, running app
; closed, app data untouched, app restarted. Same, newer or unknown versions fall through to
; the regular wizard.
Function OfferUpgrade
  ${GetOptions} $CMDLINE "/P" $0
  ${IfNot} ${Errors}
    Return
  ${EndIf}
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${IfNot} ${Errors}
    Return
  ${EndIf}

  ReadRegStr $InstalledVersion SHCTX "${POROS_UNINSTALL_KEY}" "DisplayVersion"
  ${If} $InstalledVersion == ""
    Return
  ${EndIf}

  ; The template's VERSION define is not visible here, so read this installer's own version resource.
  GetDLLVersion "$EXEPATH" $0 $1
  IntOp $2 $0 >> 16
  IntOp $3 $0 & 0xFFFF
  IntOp $1 $1 >> 16
  StrCpy $BundledVersion "$2.$3.$1"

  ${VersionCompare} $BundledVersion $InstalledVersion $0
  ${If} $0 <> 1
    Return
  ${EndIf}

  StrCpy $UpgradePrompt "$(^Name) $InstalledVersion is installed. Upgrade it to $BundledVersion?$\r$\n$\r$\nYour settings and saved connections are kept."

  ReadRegStr $1 SHCTX "${POROS_UNINSTALL_KEY}" "MainBinaryName"
  ${If} $1 != ""
    !insertmacro RestartManager_StartSession $2
    ${If} $2 != ""
      !insertmacro RestartManager_RegisterFile $2 "$INSTDIR\$1"
      ${If} $0 = 0
        System::Call 'RSTRTMGR::RmGetList(i r2, *i .r3, *i .r4, p 0, *i .r5) i .r0'
        ${If} $0 = ${ERROR_MORE_DATA}
          StrCpy $UpgradePrompt "$UpgradePrompt$\r$\n$\r$\n$(^Name) is running and will be closed. Transfers in progress will stop."
        ${EndIf}
      ${EndIf}
      !insertmacro RestartManager_EndSession $2
    ${EndIf}
  ${EndIf}

  ${If} ${Cmd} `MessageBox MB_YESNO|MB_ICONQUESTION|MB_SETFOREGROUND "$UpgradePrompt" IDYES`
    Exec '"$EXEPATH" /P /UPDATE /R'
  ${EndIf}
  Quit
FunctionEnd
