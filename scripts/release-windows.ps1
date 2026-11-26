param([string]$Iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe")
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
$PreviousKey = $env:ECHO_UPDATE_PUBLIC_KEY
try {
    $env:ECHO_UPDATE_PUBLIC_KEY = (Get-Content packaging/echo-update.pub)[1]
    & "$PSScriptRoot/build-windows.ps1" -Installer -Iscc $Iscc
} finally {
    $env:ECHO_UPDATE_PUBLIC_KEY = $PreviousKey
    Pop-Location
}
