; Установщик нативной версии Silo Player. Ставится отдельно от обычной версии (своя папка, свои ярлыки, свои данные).
Unicode true
!include "MUI2.nsh"

!define APP "Silo Player Native"
!define VER "1.3.2"
!define EXE "silo-native.exe"
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SiloPlayerNative"
!ifndef SRC
  !define SRC "..\..\target-native\release"
!endif
!ifndef OUT
  !define OUT "..\..\target-native\release\bundle"
!endif

Name "${APP}"
OutFile "${OUT}\Silo Player Native_${VER}_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\SiloPlayerNative"
InstallDirRegKey HKCU "${UNKEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "Silo Player · native"

!define MUI_ICON "..\assets\icon.ico"
!define MUI_UNICON "..\assets\icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Запустить Silo Player (native)"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "English"

VIProductVersion "${VER}.0"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "ProductName" "${APP}"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "FileDescription" "Установщик Silo Player (native)"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "FileVersion" "${VER}"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "LegalCopyright" "© 2026 Silo"

Section "Silo Player Native"
  ; закрыть запущенную нативную версию (обычную версию не трогаем)
  nsExec::Exec 'taskkill /IM ${EXE} /F'
  ; подождать, пока закрытая программа отпустит файл (иначе exe не заменится)
  Sleep 2000
  SetOverwrite try
  SetOutPath "$INSTDIR"
  File "${SRC}\${EXE}"
  ; лицензия и список сторонних компонентов лежат рядом с программой
  File "..\..\LICENSE"
  File "..\THIRD_PARTY_NATIVE.md"
  File "..\RELEASE_NOTES_1.3.2.md"
  ; встроенный yt-dlp (режим «Библиотека yt-dlp»): программа находит его рядом с собой
  !if /FileExists "bin\yt-dlp\yt-dlp.exe"
    SetOutPath "$INSTDIR\yt-dlp"
    File /r "bin\yt-dlp\*.*"
    SetOutPath "$INSTDIR"
  !endif
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortCut "$SMPROGRAMS\Silo Player (native).lnk" "$INSTDIR\${EXE}"
  CreateShortCut "$DESKTOP\Silo Player (native).lnk" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${UNKEY}" "DisplayName" "Silo Player (native)"
  WriteRegStr HKCU "${UNKEY}" "DisplayVersion" "${VER}"
  WriteRegStr HKCU "${UNKEY}" "Publisher" "Silo"
  WriteRegStr HKCU "${UNKEY}" "DisplayIcon" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${UNKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNKEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /IM ${EXE} /F'
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\yt-dlp.exe"
  RMDir /r "$INSTDIR\yt-dlp"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NATIVE.md"
  Delete "$INSTDIR\RELEASE_NOTES_1.3.2.md"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Silo Player (native).lnk"
  Delete "$DESKTOP\Silo Player (native).lnk"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "SiloPlayerNative"
  DeleteRegKey HKCU "${UNKEY}"
  ; данные (%APPDATA%\app.silo.player.native) остаются: плейлисты и лайки не пропадут при переустановке
SectionEnd
