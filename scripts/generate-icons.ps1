# Regenerate every file in src-tauri/icons from a single source PNG.
#
# Delegates to `cargo tauri icon`, which is what Tauri uses internally and
# produces the exact set the project ships: 32x32.png, 128x128.png,
# 128x128@2x.png, the Square*Logo Store PNGs, icon.ico, icon.icns, icon.png.
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
$iconsDir = Join-Path $repoRoot "src-tauri\icons"

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

Write-Host ""
Write-Host "Done. Generated files:"
Get-ChildItem $iconsDir | Sort-Object Name | Format-Table Name, Length -AutoSize
