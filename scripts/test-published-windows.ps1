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
$env:ECHO_SMOKE_REPORT = Join-Path $env:RUNNER_TEMP 'echo-transfer.txt'
$Install = Join-Path $env:RUNNER_TEMP 'Echo 更新实测'
$Exe = Join-Path $Install 'Echo.exe'
New-Item -ItemType Directory $env:ECHO_DATA_DIR | Out-Null
$Sentinel = Join-Path $env:ECHO_DATA_DIR 'existing-note.md'
Set-Content $Sentinel '升级前的笔记' -Encoding utf8NoBOM
$Before = (Get-FileHash $Sentinel).Hash

# Only the local predecessor gets the driver. Use the production check,
# download, signature verifier, staging, helper handshake and installer.
$Entry = 'fn main() {'
$Driver = @'
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--verify-published-update") {
        update::verify_published_update().expect("test predecessor could not update");
        return;
    }
'@
$Main = Get-Content src/main.rs -Raw
if (-not $Main.Contains($Entry)) { throw 'Cannot find Echo entry point' }
[IO.File]::WriteAllText("$Root/src/main.rs", $Main.Replace($Entry, $Driver))
@'
pub fn verify_published_update() -> Result<(), String> {
    let expected = std::env::var("ECHO_EXPECT_RELEASE").map_err(|e| e.to_string())?;
    let m = transport::check()?.ok_or("Expected update for predecessor")?;
    if m.version != expected { return Err(format!("Expected {expected}, got {}", m.version)); }
    let progress = std::cell::RefCell::new(Vec::new());
    let p = native::prepare(m, &|n| progress.borrow_mut().push(n))?;
    let progress = progress.into_inner();
    if progress.last() != Some(&100) || !progress.iter().any(|n| (1..100).contains(n)) {
        return Err(format!("Incomplete download progress: {progress:?}"));
    }
    let installer = p.launch()?;
    std::fs::write(std::env::var_os("ECHO_SMOKE_REPORT").expect("report path"),
        format!("Verified {}. Progress: {progress:?}", expected)).map_err(|e| e.to_string())?;
    installer.commit()
}
'@ | Add-Content src/update/mod.rs -Encoding utf8NoBOM
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
$env:ECHO_EXPECT_RELEASE = $Version
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
$Client = Start-Process $Exe -ArgumentList '--verify-published-update' -PassThru
if (-not $Client.WaitForExit(240000) -or $Client.ExitCode -ne 0) { throw "Old client failed: $($Client.ExitCode)" }
Get-Content $env:ECHO_SMOKE_REPORT
for ($i = 0; $i -lt 90; $i++) {
    $Running = Get-Process -Name Echo -ErrorAction SilentlyContinue | Where-Object Path -eq $Exe
    if ($Running -and (Test-Path "$env:ECHO_DATA_DIR/.echo/index.json") -and (Get-FileHash $Exe).Hash -ne $Old) {
        if ((Get-FileHash $Sentinel).Hash -ne $Before) { throw 'Library sentinel changed' }
        $Installed = (Get-Item $Exe).VersionInfo
        if ("$($Installed.FileMajorPart).$($Installed.FileMinorPart).$($Installed.FileBuildPart)" -ne $Version) {
            throw "Unexpected installed version: $($Installed.FileVersion)"
        }
        Write-Host "PASS: Windows x64 $Version downloaded, verified, installed and relaunched; existing note preserved."
        exit 0
    }
    Start-Sleep -Seconds 1
}
throw 'The published application did not complete startup after updating'
