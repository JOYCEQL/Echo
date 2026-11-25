param(
    [switch]$Installer,
    [string]$Iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Run this script with PowerShell 7 on Windows.' }
$Root = Split-Path $PSScriptRoot -Parent
Push-Location $Root
try {
    $Metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $Version = ($Metadata.packages | Where-Object name -eq 'echo').version
    $Target = 'x86_64-pc-windows-msvc'
    cargo build --release --locked --target $Target
    if ($LASTEXITCODE -ne 0) { throw 'Windows build failed' }
    $Name = "Echo-$Version-windows-x64"
    $Dist = Join-Path $Root 'dist'
    $Package = Join-Path $Dist $Name
    New-Item -ItemType Directory -Force $Package | Out-Null
    Copy-Item (Join-Path $Metadata.target_directory "$Target/release/echo.exe") (Join-Path $Package 'Echo.exe') -Force
    Copy-Item (Join-Path $Root 'packaging/windows/README.txt') $Package -Force
    Copy-Item (Join-Path $Root 'licenses') $Package -Recurse -Force
    Copy-Item (Join-Path $Root 'LICENSE') $Package -Force
    Copy-Item (Join-Path $Root 'THIRD_PARTY.md') $Package -Force
    Compress-Archive -Path "$Package/*" -DestinationPath "$Dist/$Name.zip" -Force
    if ($Installer) {
        if (-not (Test-Path $Iscc -PathType Leaf)) { throw "Install Inno Setup 6, or pass -Iscc: $Iscc" }
        & $Iscc "/DAppVersion=$Version" "/DSourceDir=$Package" "/DOutputDir=$Dist" (Join-Path $Root 'packaging/windows/Echo.iss')
        if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }
    }
    Write-Host "Windows artifacts: $Dist"
} finally { Pop-Location }
