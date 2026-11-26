# Run only inside the disposable candidate checkout of published-update.yml.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or $env:GITHUB_ACTIONS -ne 'true' -or (Split-Path (Get-Location) -Leaf) -ne 'candidate') {
    throw 'Use the Verify published Windows update workflow.'
}
$Root = (Get-Location).Path
$Version = ((cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages | Where-Object name -eq 'echo').version
if ($LASTEXITCODE -ne 0 -or $Version -notmatch '^\d+\.\d+\.\d+$' -or $Version -eq '0.0.0') { throw 'Expected a stable release version' }
$env:ECHO_DATA_DIR = Join-Path $env:RUNNER_TEMP 'Echo QA Library'
$Install = Join-Path $env:RUNNER_TEMP 'Echo 更新实测'
$Exe = Join-Path $Install 'Echo.exe'
New-Item -ItemType Directory $env:ECHO_DATA_DIR | Out-Null
$Sentinel = Join-Path $env:ECHO_DATA_DIR 'existing-note.md'
Set-Content $Sentinel '升级前的笔记' -Encoding utf8NoBOM
$Before = (Get-FileHash $Sentinel).Hash

$Previous = $env:ECHO_PREVIOUS_VERSION
if ($Previous -notmatch '^\d+\.\d+\.\d+$' -or [version]$Previous -ge [version]$Version) {
    throw 'Expected a published predecessor older than the candidate'
}
$Filename = "Echo-$Previous-windows-x64-setup.exe"
$Setup = Join-Path $env:RUNNER_TEMP $Filename
$Release = gh api "repos/$env:GITHUB_REPOSITORY/releases/tags/v$Previous" | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $Release.draft) { throw 'Expected a published GitHub release' }
$Asset = $Release.assets | Where-Object name -eq $Filename
Invoke-WebRequest "https://echo-downloads.magicv.art/v$Previous/$Filename" -OutFile $Setup
if (!$Asset -or $Asset.digest -ne "sha256:$((Get-FileHash $Setup).Hash)") {
    throw 'Public installer digest did not match the official GitHub asset'
}
Write-Host "Verified published v$Previous installer: $($Asset.digest)"
$SettingsDir = New-Item -ItemType Directory (Join-Path $env:ECHO_DATA_DIR '.echo')
Set-Content (Join-Path $SettingsDir.FullName 'settings.json') '{"automatic_updates":"false","ui_language":"en"}' -Encoding utf8NoBOM
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
function Invoke-EchoAction($Client, [string]$Id, [string]$Label) {
    $Selector = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id)
    $Deadline = [DateTime]::UtcNow.AddMinutes(3)
    $PreviousName = ''
    while ([DateTime]::UtcNow -lt $Deadline) {
        $Client.Refresh()
        if ($Client.HasExited) { throw "Echo exited before showing $Id" }
        if ($Client.MainWindowHandle -ne [IntPtr]::Zero) {
            $Window = [System.Windows.Automation.AutomationElement]::FromHandle($Client.MainWindowHandle)
            $Control = $Window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $Selector)
            if ($Control) {
                $Name = $Control.Current.Name
                if ($Name -ne $PreviousName) { Write-Host "$Id -> $Name"; $PreviousName = $Name }
                if ($Name -match $Label) {
                    $Control.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                    return
                }
            }
        }
        Start-Sleep -Milliseconds 150
    }
    throw "Timed out waiting for $Id. Last label: $PreviousName"
}
$Predecessor = (Get-Item $Exe).VersionInfo
if ("$($Predecessor.FileMajorPart).$($Predecessor.FileMinorPart).$($Predecessor.FileBuildPart)" -ne $Previous) {
    throw 'Installed predecessor version mismatch'
}
$Client = Start-Process $Exe -PassThru
$VersionPattern = [regex]::Escape($Version)
Invoke-EchoAction $Client 'settings' '^Settings'
Invoke-EchoAction $Client 'settings-tab-updates' '^Software updates$'
Invoke-EchoAction $Client 'update-action' '^Check for updates$'
Invoke-EchoAction $Client 'settings-back' '.'
Invoke-EchoAction $Client 'editor-updates' "^(下载更新|Download update) $VersionPattern$"
Invoke-EchoAction $Client 'editor-updates' "^(重启更新|Restart) $VersionPattern"
if (-not $Client.WaitForExit(120000)) { throw 'Predecessor did not exit after clicking Restart' }
for ($i = 0; $i -lt 90; $i++) {
    $Running = Get-Process -Name Echo -ErrorAction SilentlyContinue | Where-Object Path -eq $Exe
    if ($Running -and $Running.MainWindowHandle -ne [IntPtr]::Zero -and $Running.Id -ne $Client.Id -and (Test-Path "$env:ECHO_DATA_DIR/.echo/index.json") -and (Get-FileHash $Exe).Hash -ne $Old) {
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
