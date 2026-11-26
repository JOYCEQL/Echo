param([Parameter(Mandatory)][string]$InstallDir)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Exe = (Get-Item (Join-Path $InstallDir 'Echo.exe')).FullName
# Inno can register the group in the current user's or the shared Start menu.
$ProgramFolders = @(
    [Environment]::GetFolderPath('Programs')
    [Environment]::GetFolderPath('CommonPrograms')
) | Select-Object -Unique
$Shortcuts = @(foreach ($Folder in $ProgramFolders) {
    Get-ChildItem -LiteralPath $Folder -Recurse -File -Filter 'Echo.lnk'
})
if ($Shortcuts.Count -ne 1) {
    throw "Expected one Echo shortcut in [$($ProgramFolders -join ', ')], found [$(($Shortcuts | ForEach-Object FullName) -join ', ')]"
}
$Shortcut = $Shortcuts[0].FullName
Write-Host "Testing installed shortcut $Shortcut; the process must run from $Exe"
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class EchoWindowTest {
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd, uint message, UIntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeoutW(IntPtr hwnd, uint message, UIntPtr w, IntPtr l, uint flags, uint timeout, out UIntPtr value);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hwnd, uint attribute, out int value, int size);
}
"@
function Wait-For([scriptblock]$Condition, [string]$Description) {
    $Deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $Deadline)
    Get-Process -Name Echo -ErrorAction SilentlyContinue | ForEach-Object {
        Write-Host "PID=$($_.Id) HWND=$($_.MainWindowHandle) iconic=$([EchoWindowTest]::IsIconic($_.MainWindowHandle)) title=$($_.MainWindowTitle) path=$($_.Path)"
    }
    throw "Timed out waiting for $Description"
}
function Get-Clients {
    Get-Process -Name Echo -ErrorAction SilentlyContinue | Where-Object Path -eq $Exe
}
function Test-VisibleWindow($Handle) {
    if ($Handle -eq [IntPtr]::Zero -or [EchoWindowTest]::IsIconic($Handle)) { return $false }
    $Cloaked = 0
    $Reply = [UIntPtr]::Zero
    return [EchoWindowTest]::DwmGetWindowAttribute($Handle, 14, [ref]$Cloaked, 4) -eq 0 -and $Cloaked -eq 0 -and
        [EchoWindowTest]::SendMessageTimeoutW($Handle, 0, [UIntPtr]::Zero, [IntPtr]::Zero, 2, 500, [ref]$Reply) -ne [IntPtr]::Zero
}
function Send-WindowCommand($Handle, [uint32]$Command) {
    if (![EchoWindowTest]::PostMessageW($Handle, 0x0112, [UIntPtr]::new($Command), [IntPtr]::Zero)) {
        throw 'Could not send WM_SYSCOMMAND'
    }
}
if (@(Get-Clients).Count) { throw 'The test installation is already running' }
$PreviousLibrary = $env:ECHO_DATA_DIR
$Library = [IO.Directory]::CreateTempSubdirectory('Echo-window-test-').FullName
$env:ECHO_DATA_DIR = $Library
try {
    # Launch twice before the first window is ready. Explorer opens the same shortcut target.
    Start-Process -FilePath $Shortcut
    Start-Process -FilePath $Shortcut
    Wait-For {
        $Clients = @(Get-Clients)
        # During startup Windows may briefly report a different, untitled HWND.
        $Clients.Count -eq 1 -and $Clients[0].MainWindowTitle -ceq 'Echo' -and
            (Test-VisibleWindow $Clients[0].MainWindowHandle)
    } 'one responsive window after two launches'
    $Client = Get-Clients
    $Handle = $Client.MainWindowHandle
    Write-Host "Original PID=$($Client.Id) HWND=$Handle title=$($Client.MainWindowTitle)"
    foreach ($Restore in 'system', 'shortcut') {
        Send-WindowCommand $Handle 0xF020 # SC_MINIMIZE
        Wait-For { [EchoWindowTest]::IsIconic($Handle) } 'the window to minimize'
        if ($Restore -eq 'system') {
            Send-WindowCommand $Handle 0xF120 # SC_RESTORE
        } else {
            $Launch = Start-Process -FilePath $Shortcut -PassThru
            if ($Launch) { Write-Host "Shortcut launch PID=$($Launch.Id)" }
        }
        Wait-For {
            $Clients = @(Get-Clients)
            $Clients.Count -eq 1 -and $Clients[0].Id -eq $Client.Id -and
                $Clients[0].MainWindowHandle -eq $Handle -and (Test-VisibleWindow $Handle)
        } "one $Restore action to restore the original window"
    }
    if (!$Client.CloseMainWindow() -or !$Client.WaitForExit(15000)) { throw 'Echo did not close' }
} finally {
    Get-Clients | Stop-Process -Force
    Remove-Item -Recurse -Force -LiteralPath $Library
    $env:ECHO_DATA_DIR = $PreviousLibrary
}
