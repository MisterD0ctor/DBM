; Windows installer for Death by MPV.
;
; Per-user by design. It installs into %LOCALAPPDATA%\Programs, writes only
; under HKCU, and never asks for administrator rights — the same shape as
; VS Code's user setup or Discord's. A personal video player has no business
; showing a UAC prompt, and a machine-wide install would also need elevation
; to update, which is the one thing that should be frictionless.
;
; It registers as *a* player for video files rather than making itself the
; default. Windows 10 and later deliberately block an installer from seizing
; a file type; the correct route is to advertise the application and let the
; person choose it in Settings, which is what the Capabilities and
; OpenWithProgids keys below do.
;
; Built by packaging\build-installer.ps1, which stages the payload and passes
; in the version. Nothing here is generated: this is the whole installer.

Unicode true

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!define NAME "Death by MPV"
!define SLUG "DeathByMPV"
!define EXE "dbm-player.exe"
!define PUBLISHER "MisterD0ctor"
!define HOMEPAGE "https://github.com/MisterD0ctor/DBM"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SLUG}"

; Both are passed in by the build script; the defaults only exist so the
; script can be compiled by hand while working on it.
!ifndef VERSION
    !define VERSION "0.0.0"
!endif
!ifndef STAGE
    !define STAGE "..\target\release"
!endif
!ifndef OUTFILE
    !define OUTFILE "death-by-mpv-setup.exe"
!endif

Name "${NAME}"
OutFile "${OUTFILE}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\${NAME}"
InstallDirRegKey HKCU "Software\${SLUG}" "InstallDir"
SetCompressor /SOLID lzma
BrandingText "${NAME} ${VERSION}"

VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "FileDescription" "${NAME} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 ${PUBLISHER}. GPLv3 or later."
VIProductVersion "${VERSION}.0"

!define MUI_ICON "..\crates\player\icons\icon.ico"
!define MUI_UNICON "..\crates\player\icons\icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Open ${NAME}"

; No welcome page: it is a page whose only content is that there is a page.
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------------------------------
; The file types the player knows how to open.
;
; This list must match `VIDEO_EXTENSIONS` in crates/player/src/playlist.rs —
; offering to open a file the player would then refuse to scan is a lie the
; installer tells on the player's behalf. A test in that module reads this
; file and fails if the two drift apart.
; ---------------------------------------------------------------------------

!macro RegisterExtension EXT
    WriteRegStr HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${SLUG}.Video" ""
    WriteRegStr HKCU "Software\Classes\Applications\${EXE}\SupportedTypes" ".${EXT}" ""
    WriteRegStr HKCU "Software\${SLUG}\Capabilities\FileAssociations" ".${EXT}" "${SLUG}.Video"
!macroend

!macro UnregisterExtension EXT
    DeleteRegValue HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${SLUG}.Video"
!macroend

!macro EveryExtension MACRO
    !insertmacro ${MACRO} "mp4"
    !insertmacro ${MACRO} "mkv"
    !insertmacro ${MACRO} "avi"
    !insertmacro ${MACRO} "mov"
    !insertmacro ${MACRO} "wmv"
    !insertmacro ${MACRO} "flv"
    !insertmacro ${MACRO} "webm"
    !insertmacro ${MACRO} "m4v"
    !insertmacro ${MACRO} "mpg"
    !insertmacro ${MACRO} "mpeg"
    !insertmacro ${MACRO} "ts"
    !insertmacro ${MACRO} "m2ts"
!macroend

; ---------------------------------------------------------------------------
; Install
; ---------------------------------------------------------------------------

Section "${NAME}" SecMain
    SectionIn RO
    SetShellVarContext current

    ; A running player holds its own executable open, and Windows will not
    ; let it be replaced. Better to say so than to fail halfway through.
    retry:
    ${If} ${FileExists} "$INSTDIR\${EXE}"
        ClearErrors
        Delete "$INSTDIR\${EXE}"
        ${If} ${Errors}
            MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
                "${NAME} is running. Close it, then press Retry." \
                IDRETRY retry
            Abort "Cancelled: ${NAME} is still running."
        ${EndIf}
    ${EndIf}

    SetOutPath "$INSTDIR"
    File "${STAGE}\${EXE}"
    ; The two the player loads at runtime. It looks beside its own executable
    ; first, which is exactly here.
    File "${STAGE}\libmpv-2.dll"
    File "${STAGE}\ffmpeg.exe"
    ; The licence travels with the binaries. Both shipped libraries are
    ; GPLv3-or-later, and conveying them without their terms is the one
    ; thing the licence actually forbids.
    File "${STAGE}\LICENSE.txt"
    File "${STAGE}\THIRD-PARTY.txt"

    WriteRegStr HKCU "Software\${SLUG}" "InstallDir" "$INSTDIR"
    WriteUninstaller "$INSTDIR\uninstall.exe"

    CreateShortcut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\${EXE}"

    ; Add or remove programs.
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${NAME}"
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${EXE},0"
    WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "${HOMEPAGE}"
    WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Open video files with it" SecAssoc
    SetShellVarContext current

    ; The type itself: what it is called, what it looks like, and how to open
    ; one. `%1` in quotes because film directories have spaces in them.
    WriteRegStr HKCU "Software\Classes\${SLUG}.Video" "" "Video file"
    WriteRegStr HKCU "Software\Classes\${SLUG}.Video\DefaultIcon" "" "$INSTDIR\${EXE},0"
    WriteRegStr HKCU "Software\Classes\${SLUG}.Video\shell\open\command" "" \
        '"$INSTDIR\${EXE}" "%1"'

    ; The application, so it appears under "Open with" for anything at all.
    WriteRegStr HKCU "Software\Classes\Applications\${EXE}" "FriendlyAppName" "${NAME}"
    WriteRegStr HKCU "Software\Classes\Applications\${EXE}\shell\open\command" "" \
        '"$INSTDIR\${EXE}" "%1"'

    ; And in Settings > Default apps, where the choice is the person's to
    ; make. Advertising this is the most an installer is allowed to do.
    WriteRegStr HKCU "Software\${SLUG}\Capabilities" "ApplicationName" "${NAME}"
    WriteRegStr HKCU "Software\${SLUG}\Capabilities" "ApplicationDescription" \
        "A video player built on mpv"
    WriteRegStr HKCU "Software\RegisteredApplications" "${NAME}" \
        "Software\${SLUG}\Capabilities"

    !insertmacro EveryExtension RegisterExtension
SectionEnd

Section "Desktop shortcut" SecDesktop
    SetShellVarContext current
    CreateShortcut "$DESKTOP\${NAME}.lnk" "$INSTDIR\${EXE}"
SectionEnd

LangString DESC_SecMain ${LANG_ENGLISH} \
    "The player, and the two binaries it ships with: libmpv for playback and \
     ffmpeg for seek-preview thumbnails."
LangString DESC_SecAssoc ${LANG_ENGLISH} \
    "Offer ${NAME} in the Open with menu for video files, and list it in \
     Settings > Default apps. It does not take over any file type by itself."
LangString DESC_SecDesktop ${LANG_ENGLISH} "Put a shortcut on the desktop."

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
    !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} $(DESC_SecMain)
    !insertmacro MUI_DESCRIPTION_TEXT ${SecAssoc} $(DESC_SecAssoc)
    !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} $(DESC_SecDesktop)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; ---------------------------------------------------------------------------
; Uninstall
; ---------------------------------------------------------------------------

Section "Uninstall"
    SetShellVarContext current

    Delete "$INSTDIR\${EXE}"
    Delete "$INSTDIR\libmpv-2.dll"
    Delete "$INSTDIR\ffmpeg.exe"
    Delete "$INSTDIR\LICENSE.txt"
    Delete "$INSTDIR\THIRD-PARTY.txt"
    Delete "$INSTDIR\uninstall.exe"
    ; Only if empty: anything else in there was put there by hand, and
    ; deleting somebody's files because they shared a folder is not on.
    RMDir "$INSTDIR"

    Delete "$SMPROGRAMS\${NAME}.lnk"
    Delete "$DESKTOP\${NAME}.lnk"

    DeleteRegKey HKCU "${UNINST_KEY}"
    DeleteRegKey HKCU "Software\${SLUG}"
    DeleteRegKey HKCU "Software\Classes\${SLUG}.Video"
    DeleteRegKey HKCU "Software\Classes\Applications\${EXE}"
    DeleteRegValue HKCU "Software\RegisteredApplications" "${NAME}"
    !insertmacro EveryExtension UnregisterExtension

    ; Settings, resume positions, the duration cache and the thumbnail cache.
    ; Kept unless asked, because an uninstall is often a reinstall, and
    ; finding every film back at zero afterwards is its own small disaster.
    ; Silent uninstalls keep it: no answer is not the same as yes.
    ${IfNot} ${Silent}
        MessageBox MB_YESNO|MB_ICONQUESTION \
            "Also remove your settings, watch history and thumbnail cache?" \
            /SD IDNO IDNO keep_data
        RMDir /r "$APPDATA\${NAME}"
        keep_data:
    ${EndIf}
SectionEnd
