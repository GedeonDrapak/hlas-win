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
!ifndef BIN_DIR
  !define BIN_DIR "..\target\release"
!endif
!ifdef RUNTIME_DIR
  !define HAS_GNU_RUNTIME
!endif

Name "${APPNAME}"
OutFile "Hlas-setup.exe"
; A dictation helper should install without elevation. All its user data and
; launch-at-login registry entry are already per-user.
InstallDir "$LOCALAPPDATA\${APPNAME}"
InstallDirRegKey HKCU "Software\${APPNAME}" "InstallDir"
RequestExecutionLevel user
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
  File "${BIN_DIR}\hlas.exe"
  File "..\assets\hlas.ico"
  !ifdef HAS_GNU_RUNTIME
    ; Cross-compiled GNU builds need the complete runtime beside hlas.exe.
    File "${RUNTIME_DIR}\libstdc++-6.dll"
    File "${RUNTIME_DIR}\libgcc_s_seh-1.dll"
    File "${RUNTIME_DIR}\..\bin\libwinpthread-1.dll"
  !endif

  CreateShortCut "$SMPROGRAMS\${APPNAME}.lnk" "$INSTDIR\hlas.exe" "" "$INSTDIR\hlas.ico"

  WriteRegStr HKCU "Software\${APPNAME}" "InstallDir" "$INSTDIR"

  ; Add/Remove Programs metadata.
  !define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APPNAME} - ${DESCRIPTION}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\hlas.ico"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${COMPANY}"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\hlas.exe"
  Delete "$INSTDIR\hlas.ico"
  Delete "$INSTDIR\libstdc++-6.dll"
  Delete "$INSTDIR\libgcc_s_seh-1.dll"
  Delete "$INSTDIR\libwinpthread-1.dll"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APPNAME}.lnk"

  ; Remove the per-user launch-at-login entry the app may have written.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Hlas"

  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"
  DeleteRegKey HKCU "Software\${APPNAME}"
SectionEnd
