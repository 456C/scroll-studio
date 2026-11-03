; -*- coding: utf-8 -*-
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

!ifndef APP_VERSION
  !error "Build with scripts/installer.ps1."
!endif
!define APP_KEY "Software\ScrollStudio\Installer"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScrollStudio"
!define STORY_KEY "Software\Classes\ScrollStudio.Story"

Name "Scroll Studio"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\Scroll Studio"
InstallDirRegKey HKCU "${APP_KEY}" "InstallLocation"
RequestExecutionLevel user
ManifestDPIAware true
ManifestLongPathAware true
SetCompressor /SOLID zlib
SetDatablockOptimize on
CRCCheck force
AllowSkipFiles off
ShowInstDetails nevershow
ShowUninstDetails nevershow
BrandingText "Scroll Studio ${APP_VERSION}"
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey "ProductName" "Scroll Studio"
VIAddVersionKey "FileDescription" "Scroll Studio Setup"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "LegalCopyright" "Scroll Studio contributors"

!define MUI_ICON "${APP_ICON}"
!define MUI_UNICON "${APP_ICON}"
!define MUI_COMPONENTSPAGE_NODESC
!define MUI_DIRECTORYPAGE_TEXT_TOP "Choose where to install Scroll Studio for your Windows account."
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\Scroll Studio.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Scroll Studio"
!insertmacro MUI_PAGE_FINISH
!define MUI_UNCONFIRMPAGE_TEXT_TOP "Remove Scroll Studio from this computer? Saved stories, samples, preferences and recovery copies will be kept."
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  SetShellVarContext current
  SetRegView 64
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Scroll Studio requires 64-bit Windows." /SD IDOK
    SetErrorLevel 1633
    Quit
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "Scroll Studio requires Windows 10 or later." /SD IDOK
    SetErrorLevel 1633
    Quit
  ${EndIf}
FunctionEnd

Section "Scroll Studio" Main
  SectionIn RO
  SetShellVarContext current
  SetRegView 64
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "${PACKAGE_DIR}\Scroll Studio.exe"
  File "${PACKAGE_DIR}\README.md"
  File "${PACKAGE_DIR}\LICENSE"
  File "${PACKAGE_DIR}\font-research.md"
  File "${PACKAGE_DIR}\icon-generation.md"
  File "${PACKAGE_DIR}\ui-review.md"
  File "${PACKAGE_DIR}\workflow-research.md"
  File "${PACKAGE_DIR}\painting-performance.md"
  File "${PACKAGE_DIR}\Karla-OFL.txt"
  File "${PACKAGE_DIR}\ComicNeue-OFL.txt"
  File "${PACKAGE_DIR}\PatrickHand-OFL.txt"
  File "${PACKAGE_DIR}\DeliusUnicase-OFL.txt"
  File "${PACKAGE_DIR}\Lato-OFL.txt"
  File "${PACKAGE_DIR}\BalsamiqSans-OFL.txt"
  File "${PACKAGE_DIR}\KomikaHand-LICENSE.txt"
  File /oname=NSIS-LICENSE.txt "${NSISDIR}\COPYING"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\Scroll Studio"
  CreateShortcut "$SMPROGRAMS\Scroll Studio\Scroll Studio.lnk" "$INSTDIR\Scroll Studio.exe"
  WriteRegStr HKCU "${APP_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "Scroll Studio"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Scroll Studio contributors"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" '"$INSTDIR\Scroll Studio.exe",0'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" ${INSTALL_KIB}
SectionEnd

Section "Sample stories" Samples
  SetOutPath "$INSTDIR\Samples"
  ; Existing samples may contain the user's edits. Never replace or uninstall them.
  SetOverwrite off
  File "${PACKAGE_DIR}\Lettering playground.scro"
  File "${PACKAGE_DIR}\Bubble presets playground.scro"
  File "${PACKAGE_DIR}\Character studio playground.scro"
  File "${PACKAGE_DIR}\Layout playground.scro"
  File "${PACKAGE_DIR}\After the rain.scro"
  SetOverwrite on
  CreateShortcut "$SMPROGRAMS\Scroll Studio\Sample stories.lnk" "$INSTDIR\Samples"
SectionEnd

Section "Windows Open with integration" FileTypes
  ; Advertise support without changing .scro defaults or Windows UserChoice.
  WriteRegStr HKCU "${STORY_KEY}" "" "Scroll Studio story"
  WriteRegStr HKCU "${STORY_KEY}\DefaultIcon" "" '"$INSTDIR\Scroll Studio.exe",0'
  WriteRegStr HKCU "${STORY_KEY}\shell\open\command" "" '"$INSTDIR\Scroll Studio.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\.scro\OpenWithProgids" "ScrollStudio.Story" ""
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

Section /o "Desktop shortcut" Desktop
  CreateShortcut "$DESKTOP\Scroll Studio.lnk" "$INSTDIR\Scroll Studio.exe"
SectionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
  ReadRegStr $0 HKCU "${APP_KEY}" "InstallLocation"
  ${If} $0 != $INSTDIR
    MessageBox MB_OK|MB_ICONSTOP "Use the uninstaller in the current Scroll Studio installation folder." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
FunctionEnd

Section "Uninstall"
  ; Explicit app files only: never recurse into saved stories or user data.
  ClearErrors
  Delete "$INSTDIR\Scroll Studio.exe"
  ${If} ${Errors}
    MessageBox MB_OK|MB_ICONSTOP "Close Scroll Studio, then run the uninstaller again." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\font-research.md"
  Delete "$INSTDIR\icon-generation.md"
  Delete "$INSTDIR\ui-review.md"
  Delete "$INSTDIR\workflow-research.md"
  Delete "$INSTDIR\painting-performance.md"
  Delete "$INSTDIR\Karla-OFL.txt"
  Delete "$INSTDIR\ComicNeue-OFL.txt"
  Delete "$INSTDIR\PatrickHand-OFL.txt"
  Delete "$INSTDIR\DeliusUnicase-OFL.txt"
  Delete "$INSTDIR\Lato-OFL.txt"
  Delete "$INSTDIR\BalsamiqSans-OFL.txt"
  Delete "$INSTDIR\KomikaHand-LICENSE.txt"
  Delete "$INSTDIR\NSIS-LICENSE.txt"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\Scroll Studio\Scroll Studio.lnk"
  Delete "$SMPROGRAMS\Scroll Studio\Sample stories.lnk"
  Delete "$DESKTOP\Scroll Studio.lnk"
  RMDir "$SMPROGRAMS\Scroll Studio"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "${APP_KEY}"
  DeleteRegKey /ifempty HKCU "Software\ScrollStudio"
  DeleteRegKey HKCU "${STORY_KEY}"
  DeleteRegValue HKCU "Software\Classes\.scro\OpenWithProgids" "ScrollStudio.Story"
  DeleteRegKey /ifempty HKCU "Software\Classes\.scro\OpenWithProgids"
  DeleteRegKey /ifempty HKCU "Software\Classes\.scro"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd
