# Build the Windows installer.
#
#   .\packaging\build-installer.ps1              # release build, then package
#   .\packaging\build-installer.ps1 -SkipBuild   # package what is already built
#
# The staging directory is the whole point: the player looks for libmpv and
# ffmpeg beside its own executable first, so what the installer ships is a
# directory laid out exactly as the installed one will be. ffmpeg loses its
# target-triple suffix on the way in — that spelling was Tauri's requirement,
# and the lookup takes the plain name a packaged build uses.

[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [string]$Version
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $repo "target\package"
$out = Join-Path $repo "target\installer"

if (-not $Version) {
    # One source for the version: the crate's own manifest.
    $manifest = Get-Content (Join-Path $repo "crates\player\Cargo.toml") -Raw
    if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw "no version in crates/player/Cargo.toml"
    }
    $Version = $Matches[1]
}

# makensis is on PATH on GitHub's Windows runners. Locally it is usually
# wherever NSIS was installed, including the copy Tauri downloads for itself.
$nsis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
if (-not $nsis) {
    $candidates = @(
        "$env:LOCALAPPDATA\tauri\NSIS\Bin\makensis.exe",
        "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
        "$env:ProgramFiles\NSIS\makensis.exe"
    )
    $nsis = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $nsis) {
    throw "makensis not found. Install NSIS (winget install NSIS.NSIS) and retry."
}

if (-not $SkipBuild) {
    Write-Host "Building dbm-player $Version"
    & cargo build --release -p dbm-player
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}

Write-Host "Staging into $stage"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
New-Item -ItemType Directory -Force $out | Out-Null

Copy-Item (Join-Path $repo "target\release\dbm-player.exe") $stage
Copy-Item (Join-Path $repo "crates\player\vendor\libmpv-2.dll") $stage
Copy-Item (Join-Path $repo "crates\player\vendor\ffmpeg-x86_64-pc-windows-msvc.exe") `
    (Join-Path $stage "ffmpeg.exe")
# The licence goes with them, into the installer and into the portable
# zip alike. Named .txt so double-clicking one on Windows opens it
# instead of asking what it is.
Copy-Item (Join-Path $repo "LICENSE") (Join-Path $stage "LICENSE.txt")
Copy-Item (Join-Path $repo "THIRD-PARTY.md") (Join-Path $stage "THIRD-PARTY.txt")

$installer = Join-Path $out "death-by-mpv-$Version-setup.exe"
Write-Host "Packaging $installer"
# No /NOCD: makensis changes to the script's own directory, which is what
# makes the icon path inside the script resolve. Everything passed in here is
# absolute, so nothing else depends on where it runs from.
& $nsis `
    "/DVERSION=$Version" `
    "/DSTAGE=$stage" `
    "/DOUTFILE=$installer" `
    (Join-Path $PSScriptRoot "death-by-mpv.nsi")
if ($LASTEXITCODE -ne 0) { throw "makensis failed" }

$size = [math]::Round((Get-Item $installer).Length / 1MB, 1)
Write-Host "Done: $installer ($size MB)"
