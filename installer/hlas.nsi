; Hlas for Windows - NSIS installer
; Produces Hlas-setup.exe: installs the single exe, a Start Menu shortcut, and
; uninstall metadata. The Whisper model is downloaded by the app on first run,
; so the installer stays tiny.

Unicode true
!define APPNAME "Hlas"
!define COMPANY "Gedeon Drapak"
!define DESCRIPTION "Ultra-minimal dictation"
!ifndef VERSION
  !define VERSION "0.1.0"
!endif

Name "${APPNAME}"
OutFile "Hlas-setup.exe"
InstallDir "$PROGRAMFILES64\${APPNAME}"
InstallDirRegKey HKLM "Software\${APPNAME}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!define MUI_ICON "..\assets\hlas.ico"
!define MUI_UNICON "..\assets\hlas.ico"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\hlas.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Hlas"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "..\target\release\hlas.exe"
  File "..\assets\hlas.ico"

  CreateShortCut "$SMPROGRAMS\${APPNAME}.lnk" "$INSTDIR\hlas.exe" "" "$INSTDIR\hlas.ico"

  WriteRegStr HKLM "Software\${APPNAME}" "InstallDir" "$INSTDIR"

  ; Add/Remove Programs metadata.
  !define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${APPNAME} - ${DESCRIPTION}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\hlas.ico"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "${COMPANY}"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1

  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\hlas.exe"
  Delete "$INSTDIR\hlas.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APPNAME}.lnk"

  ; Remove the per-user launch-at-login entry the app may have written.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Hlas"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"
  DeleteRegKey HKLM "Software\${APPNAME}"
SectionEnd
