# Run only inside the disposable candidate checkout of published-update.yml.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or $env:GITHUB_ACTIONS -ne 'true' -or (Split-Path (Get-Location) -Leaf) -ne 'candidate') {
    throw 'Use the Verify published Windows update workflow.'
}
$Root = (Get-Location).Path
$Version = ((cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages | Where-Object name -eq 'echo').version
if ($LASTEXITCODE -ne 0 -or $Version -notmatch '^\d+\.\d+\.\d+$' -or $Version -eq '0.0.0') { throw 'Expected a stable release version' }
$env:ECHO_UPDATE_PUBLIC_KEY = (Get-Content packaging/echo-update.pub)[1]
$env:ECHO_DATA_DIR = Join-Path $env:RUNNER_TEMP 'Echo QA Library'
$Install = Join-Path $env:RUNNER_TEMP 'Echo 更新实测'
$Exe = Join-Path $Install 'Echo.exe'
New-Item -ItemType Directory $env:ECHO_DATA_DIR | Out-Null
$Sentinel = Join-Path $env:ECHO_DATA_DIR 'existing-note.md'
Set-Content $Sentinel '升级前的笔记' -Encoding utf8NoBOM
$Before = (Get-FileHash $Sentinel).Hash

# Build a normal predecessor, then operate the same sidebar controls users see.
$Assembly = Join-Path $Root 'packaging/windows/Echo.manifest'
[IO.File]::WriteAllText($Assembly, (Get-Content $Assembly -Raw).Replace("version=`"$Version.0`"", 'version="0.0.0.0"'))
$Manifest = Get-Content Cargo.toml -Raw
[IO.File]::WriteAllText("$Root/Cargo.toml", ([regex]'(?m)^version = "[^"]+"').Replace($Manifest, 'version = "0.0.0"', 1))
$Lock = Get-Content Cargo.lock -Raw
$PackageVersion = [regex]'(?m)(^name = "echo"\r?\nversion = ")[^"]+"'
if (-not $PackageVersion.IsMatch($Lock)) { throw 'Echo is missing from Cargo.lock' }
[IO.File]::WriteAllText("$Root/Cargo.lock", $PackageVersion.Replace($Lock, '${1}0.0.0"', 1))
rustup target add x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'Missing Rust target' }
& ./scripts/build-windows.ps1 -Installer
$Setup = Join-Path $Root 'dist/Echo-0.0.0-windows-x64-setup.exe'
$Init = Start-Process $Setup -ArgumentList "/VERYSILENT /NORESTART /SUPPRESSMSGBOXES /DIR=`"$Install`"" -PassThru -Wait
if ($Init.ExitCode -ne 0 -or -not (Test-Path "$Install/unins000.exe")) { throw 'Predecessor install failed' }
$Old = (Get-FileHash $Exe).Hash
$Published = $false
for ($i = 0; $i -lt 90; $i++) {
    try {
        $Feed = Invoke-RestMethod "https://echo-downloads.magicv.art/latest/update-windows-x64.json"
        $Published = $Feed.version -eq $Version
    } catch { Write-Host "Waiting for the public $Version feed" }
    if ($Published) { break }
    Start-Sleep -Seconds 20
}
if (-not $Published) { throw 'Public release feed was not ready' }
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$SidebarAction = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'editor-updates')
function Invoke-SidebarAction($Client, [string]$Label) {
    $Deadline = [DateTime]::UtcNow.AddMinutes(3)
    $Previous = ''
    while ([DateTime]::UtcNow -lt $Deadline) {
        $Client.Refresh()
        if ($Client.HasExited) { throw "Echo exited before showing $Label" }
        if ($Client.MainWindowHandle -ne [IntPtr]::Zero) {
            $Window = [System.Windows.Automation.AutomationElement]::FromHandle($Client.MainWindowHandle)
            $Control = $Window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $SidebarAction)
            if ($Control) {
                $Name = $Control.Current.Name
                if ($Name -ne $Previous) { Write-Host "Sidebar: $Name"; $Previous = $Name }
                if ($Name -match $Label) {
                    $Control.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                    return
                }
            }
        }
        Start-Sleep -Milliseconds 150
    }
    throw "Sidebar did not show $Label. Last state: $Previous"
}
$Client = Start-Process $Exe -PassThru
$VersionPattern = [regex]::Escape($Version)
Invoke-SidebarAction $Client "^(下载更新|Download update) $VersionPattern$"
Invoke-SidebarAction $Client "^(重启更新|Restart) $VersionPattern"
if (-not $Client.WaitForExit(120000)) { throw 'Predecessor did not exit after clicking Restart' }
for ($i = 0; $i -lt 90; $i++) {
    $Running = Get-Process -Name Echo -ErrorAction SilentlyContinue | Where-Object Path -eq $Exe
    if ($Running -and $Running.Id -ne $Client.Id -and (Test-Path "$env:ECHO_DATA_DIR/.echo/index.json") -and (Get-FileHash $Exe).Hash -ne $Old) {
        if ((Get-FileHash $Sentinel).Hash -ne $Before) { throw 'Library sentinel changed' }
        $Installed = (Get-Item $Exe).VersionInfo
        if ("$($Installed.FileMajorPart).$($Installed.FileMinorPart).$($Installed.FileBuildPart)" -ne $Version) {
            throw "Unexpected installed version: $($Installed.FileVersion)"
        }
        Write-Host "PASS: Windows x64 $Version downloaded and installed from the sidebar, and relaunched; existing note preserved."
        exit 0
    }
    Start-Sleep -Seconds 1
}
throw 'The published application did not complete startup after updating'
