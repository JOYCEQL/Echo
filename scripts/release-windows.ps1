param(
    [Parameter(Mandatory)][string]$Repository,
    [Parameter(Mandatory)][string]$PublicKeyFile,
    [Parameter(Mandatory)][string]$SecretKeyFile,
    [string]$Minisign = 'minisign',
    [string]$Iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($Repository -notmatch '^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' -or
    ($Repository.Split('/')[-2..-1] | Where-Object { $_ -in '.', '..' })) { throw 'Expected a public GitHub repository URL' }
$Key = @(Get-Content $PublicKeyFile | Where-Object { $_ -match '^RW[A-Za-z0-9+/=]+$' })
if ($Key.Count -ne 1) { throw 'Invalid Minisign public key file' }
$PublicKeyFile = (Resolve-Path $PublicKeyFile).Path
$SecretKeyFile = (Resolve-Path $SecretKeyFile).Path
$OldRepo = $env:ECHO_UPDATE_REPOSITORY
$OldKey = $env:ECHO_UPDATE_PUBLIC_KEY
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $env:ECHO_UPDATE_REPOSITORY = $Repository
    $env:ECHO_UPDATE_PUBLIC_KEY = $Key[0]
    & "$PSScriptRoot/build-windows.ps1" -Installer -Iscc $Iscc
    $Metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $Version = ($Metadata.packages | Where-Object name -eq 'echo').version
    $Name = "Echo-$Version-windows-x64-setup.exe"
    $Package = Join-Path (Get-Location) "dist/$Name"
    & $Minisign -S -s $SecretKeyFile -m $Package -t $Name
    if ($LASTEXITCODE -ne 0) { throw 'Signing failed' }
    & $Minisign -V -p $PublicKeyFile -m $Package
    if ($LASTEXITCODE -ne 0) { throw 'Signature verification failed' }
    $Manifest = [ordered]@{
        version = $Version; platform = 'windows-x64'; size = (Get-Item $Package).Length
        signature = Get-Content "$Package.minisig" -Raw
    }
    $Manifest | ConvertTo-Json | Set-Content 'dist/update-windows-x64.json' -Encoding utf8NoBOM
    Write-Host "Upload $Name and update-windows-x64.json to the v$Version GitHub release."
} finally {
    $env:ECHO_UPDATE_REPOSITORY = $OldRepo
    $env:ECHO_UPDATE_PUBLIC_KEY = $OldKey
    Pop-Location
}
