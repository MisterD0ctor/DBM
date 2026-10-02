# Regenerate every file in crates/player/icons from a single source PNG.
#
# Delegates to `cargo tauri icon`. The application has nothing to do with
# Tauri any more, but its icon generator is a perfectly good tool and
# installing it to regenerate a set once in a while costs nothing. It writes
# more shapes than this player uses — Store tiles, mobile launchers, a macOS
# .icns — so everything not in $Keep is deleted again afterwards. The six
# that stay are the ones something reads:
#
#   icon.ico            the executable's own icon (build.rs) and the installer's
#   128x128@2x.png      the window icon (app.slint), and the Flatpak's 256px
#   32x32.png, 64x64.png, 128x128.png, icon.png
#                       the Flatpak's hicolor sizes
#
# Usage:
#   .\scripts\generate-icons.ps1                 # uses default source path
#   .\scripts\generate-icons.ps1 -Source path    # override source PNG
#
# Source image should be square, at least 1024x1024, with transparency.

[CmdletBinding()]
param(
    [string]$Source = "$env:USERPROFILE\OneDrive\Bilder\death-by-mpv.png"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$iconsDir = Join-Path $repoRoot "crates\player\icons"
$Keep = @("32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.ico", "icon.png")

if (-not (Test-Path $Source)) {
    throw "Source image not found: $Source"
}

Write-Host "Source : $Source"
Write-Host "Target : $iconsDir"
Write-Host ""

Push-Location $repoRoot
try {
    & cargo tauri icon $Source --output $iconsDir
    if ($LASTEXITCODE -ne 0) {
        throw "cargo tauri icon exited with code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

Get-ChildItem $iconsDir |
    Where-Object { $Keep -notcontains $_.Name } |
    Remove-Item -Recurse -Force

Write-Host ""
Write-Host "Done. Kept:"
Get-ChildItem $iconsDir | Sort-Object Name | Format-Table Name, Length -AutoSize
