# Builds the release exe and publishes it as a GitHub release.
# Use when GitHub Actions is unavailable (for example a locked billing account).
#
#   powershell -ExecutionPolicy Bypass -File scripts\release.ps1 -Tag v0.1.0

param(
    [string]$Tag = "v0.1.0",
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

try {
    if (-not $SkipBuild) {
        cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }

    New-Item -ItemType Directory -Force -Path 'dist' | Out-Null
    Copy-Item 'target/release/nsound.exe' 'dist/nSound.exe' -Force
    Write-Output "built dist/nSound.exe"

    gh release view $Tag *> $null
    if ($LASTEXITCODE -ne 0) {
        gh release create $Tag 'dist/nSound.exe#nSound.exe' --title "nSound $Tag" --notes "nSound $Tag"
    } else {
        gh release upload $Tag 'dist/nSound.exe#nSound.exe' --clobber
    }
    if ($LASTEXITCODE -ne 0) { throw "gh release failed" }

    gh release view $Tag --json url --jq .url
}
finally {
    Pop-Location
}
