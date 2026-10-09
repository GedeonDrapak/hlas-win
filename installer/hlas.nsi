; Hlas for Windows - NSIS installer
; Produces Hlas-setup.exe: a per-user install (no administrator rights) of the
; single hlas.exe, a Start menu shortcut and a normal Windows uninstaller. The
; Whisper model is downloaded by the app, so the installer stays tiny.

Unicode true
!define APPNAME "Hlas"
!define COMPANY "Gedeon Drapak"
!define DESCRIPTION "Ultra-minimal dictation"
!ifndef VERSION
  !define VERSION "0.2.0"
!endif
!ifndef BIN_DIR
  !define BIN_DIR "..\target\release"
!endif
!ifdef RUNTIME_DIR
  !define HAS_GNU_RUNTIME
!endif

Name "${APPNAME}"
OutFile "Hlas-setup.exe"
; Programs live apart from user data (%LOCALAPPDATA%\Hlas: config, history,
; model, log), so reinstalling or uninstalling never touches them by accident.
InstallDir "$LOCALAPPDATA\Programs\${APPNAME}"
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APPNAME}"
VIAddVersionKey "FileDescription" "${APPNAME} installer"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${COMPANY}"
VIAddVersionKey "LegalCopyright" "MIT License"

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

!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"

Section "Install"
  ; A running Hlas holds the exe open; stop it before replacing it.
  nsExec::Exec 'taskkill /F /IM hlas.exe'
  Sleep 400

  ; 0.1.0 installed into the data folder. Remove its program files only;
  ; config, history and the model stay where they are.
  Delete "$LOCALAPPDATA\${APPNAME}\hlas.exe"
  Delete "$LOCALAPPDATA\${APPNAME}\hlas.ico"
  Delete "$LOCALAPPDATA\${APPNAME}\uninstall.exe"
  Delete "$LOCALAPPDATA\${APPNAME}\libstdc++-6.dll"
  Delete "$LOCALAPPDATA\${APPNAME}\libgcc_s_seh-1.dll"
  Delete "$LOCALAPPDATA\${APPNAME}\libwinpthread-1.dll"

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

  ; Launch-at-login written by 0.1.0 points at the old location.
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Hlas"
  StrCmp $0 "" +2
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Hlas" '"$INSTDIR\hlas.exe"'

  WriteRegStr HKCU "Software\${APPNAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APPNAME} - ${DESCRIPTION}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\hlas.ico"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${COMPANY}"
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "https://github.com/GedeonDrapak/hlas-win"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 8000
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /F /IM hlas.exe'
  Sleep 400

  Delete "$INSTDIR\hlas.exe"
  Delete "$INSTDIR\hlas.ico"
  Delete "$INSTDIR\libstdc++-6.dll"
  Delete "$INSTDIR\libgcc_s_seh-1.dll"
  Delete "$INSTDIR\libwinpthread-1.dll"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APPNAME}.lnk"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Hlas"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\${APPNAME}"

  MessageBox MB_YESNO|MB_ICONQUESTION "Also remove your Hlas settings, dictation history, saved API keys and the downloaded model (about 550 MB)?" /SD IDNO IDNO keep_data
    RMDir /r "$LOCALAPPDATA\${APPNAME}"
    RMDir /r "$APPDATA\${APPNAME}"
    nsExec::Exec 'cmdkey /delete:groq-api-key.com.gedeon.hlas'
    nsExec::Exec 'cmdkey /delete:openai-api-key.com.gedeon.hlas'
  keep_data:
SectionEnd
