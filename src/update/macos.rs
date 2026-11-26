//! Package authenticity comes from Minisign; ad-hoc code signing needs no Apple subscription.
//! Bundle replacement uses an atomic exchange on the destination volume.
use super::transport::{self, Manifest, run};
use std::os::unix::{
    ffi::OsStrExt,
    fs::{MetadataExt, PermissionsExt},
    process::CommandExt,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub struct Prepared {
    pub manifest: Manifest,
    stage: tempfile::TempDir,
    app: PathBuf,
}

pub use super::installer::Installer;

#[derive(serde::Serialize, serde::Deserialize)]
struct Plan {
    manifest: Manifest,
    app: PathBuf,
    device: u64,
    inode: u64,
}

fn plist(app: &Path, field: &str) -> Result<String, String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{field}")])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("Missing bundle field: {field}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn verify_code(app: &Path) -> Result<(), String> {
    run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app))
}

pub fn installed_app() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let app = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("App bundle not found")?;
    if app.extension().is_none_or(|ext| ext != "app")
        || executable != app.join("Contents/MacOS/Echo")
    {
        return Err("Updates require an installed Echo.app".into());
    }
    if app
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Err("Move Echo to Applications before updating".into());
    }
    if plist(app, "CFBundleIdentifier")? != "local.echo.macos" {
        return Err("Application identifier mismatch".into());
    }
    verify_code(app)?;
    Ok(app.to_owned())
}

fn verify_app(new: &Path, app: &Path, manifest: &Manifest) -> Result<(), String> {
    // Called only after verifying the entire archive with the embedded release key.
    verify_code(new)?;
    fn team(app: &Path) -> Result<Option<String>, String> {
        let details = Command::new("/usr/bin/codesign").args(["-d", "--verbose=4"]).arg(app)
            .output().map_err(|err| err.to_string())?;
        if !details.status.success() { return Err("Could not identify app signature".into()); }
        let description = String::from_utf8_lossy(&details.stderr);
        let id = description.lines().find_map(|line| line.strip_prefix("TeamIdentifier="))
            .ok_or("Missing signing team information")?;
        Ok((id != "not set").then(|| id.to_string()))
    }
    if let Some(installed) = team(app)? && team(new)?.as_ref() != Some(&installed) {
        return Err("Update was signed by a different Apple developer team".into());
    }
    if plist(new, "CFBundleIdentifier")? != plist(app, "CFBundleIdentifier")?
        || plist(new, "CFBundleShortVersionString")? != manifest.version
        || plist(new, "CFBundleExecutable")? != "Echo"
    {
        return Err("Update bundle identity/version mismatch".into());
    }
    let arch = match transport::platform()? {
        "macos-arm64" => "arm64",
        "macos-x86_64" => "x86_64",
        _ => return Err("Unsupported architecture".into()),
    };
    run(Command::new("/usr/bin/lipo")
        .arg(new.join("Contents/MacOS/Echo"))
        .args(["-verify_arch", arch]))
}

fn create_stage(parent: &Path) -> Result<tempfile::TempDir, String> {
    tempfile::Builder::new()
        .prefix(".echo-update-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)
        .map_err(|e| format!("Cannot prepare an update in {}: {e}", parent.display()))
}

pub fn prepare(manifest: Manifest, progress: &dyn Fn(u8)) -> Result<Prepared, String> {
    let app = installed_app()?;
    let parent = app.parent().ok_or("Application parent not found")?;
    let stage = create_stage(parent)?;
    transport::download(&manifest, &stage.path().join("package.zip"), progress)?;
    let unpacked = stage.path().join("unpacked");
    run(Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(stage.path().join("package.zip"))
        .arg(&unpacked))?;
    verify_app(&unpacked.join("Echo.app"), &app, &manifest)?;
    let metadata = fs::metadata(&app).map_err(|e| e.to_string())?;
    let plan = Plan {
        manifest: manifest.clone(),
        app: app.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    fs::write(
        stage.path().join("plan.json"),
        serde_json::to_vec(&plan).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(Prepared {
        manifest,
        stage,
        app,
    })
}

impl Prepared {
    pub fn launch(self) -> Result<Installer, String> {
        let log =
            fs::File::create(self.stage.path().join("install.log")).map_err(|e| e.to_string())?;
        // The helper starts before quitting, but cannot mutate the app until our PID exits.
        let mut command = Command::new(self.app.join("Contents/MacOS/Echo"));
        command
            .arg("--apply-update")
            .arg(self.stage.path())
            .arg(std::process::id().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .process_group(0);
        if let Some(root) = std::env::var_os("ECHO_DATA_DIR") {
            command.env("ECHO_DATA_DIR", root);
        }
        let child = command
            .spawn()
            .map_err(|e| format!("Could not start update installer: {e}"))?;
        Installer::ready(child, self.stage)
    }
}

fn exchange(a: &Path, b: &Path) -> Result<(), String> {
    let a = std::ffi::CString::new(a.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let b = std::ffi::CString::new(b.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // Either both bundle names exchange, or neither changes. No copy/delete fallback.
    let status = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            a.as_ptr(),
            libc::AT_FDCWD,
            b.as_ptr(),
            libc::RENAME_SWAP,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

fn apply(stage: &Path, parent_pid: i32) -> Result<(), String> {
    if parent_pid <= 1 || parent_pid == std::process::id() as i32 {
        return Err("Invalid parent process".into());
    }
    let stage = stage.canonicalize().map_err(|e| e.to_string())?;
    let metadata = fs::metadata(&stage).map_err(|e| e.to_string())?;
    if metadata.uid() != unsafe { libc::getuid() } || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Update staging directory is not private".into());
    }
    let plan: Plan =
        serde_json::from_slice(&fs::read(stage.join("plan.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if plan.app != installed_app()? || stage.parent() != plan.app.parent() {
        return Err("Update target mismatch".into());
    }
    plan.manifest.validate(transport::platform()?)?;
    if transport::version(&plan.manifest.version)? <= transport::version(env!("CARGO_PKG_VERSION"))?
    {
        return Err("Update is not newer".into());
    }
    plan.manifest
        .verify(&stage.join("package.zip"), &transport::public_key()?)?;
    let new = stage.join("unpacked/Echo.app");
    verify_app(&new, &plan.app, &plan.manifest)?;
    // Parent performs the final save only after our verification succeeds.
    // EOF cancels installation if saving fails or the parent goes away early.
    println!("READY");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut decision = String::new();
    std::io::stdin()
        .read_line(&mut decision)
        .map_err(|e| e.to_string())?;
    if decision != "INSTALL\n" {
        return Err("Installation was not committed".into());
    }
    let started = Instant::now();
    while unsafe { libc::kill(parent_pid, 0) } == 0 {
        if started.elapsed() > Duration::from_secs(120) {
            return Err("Application did not exit; update cancelled".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
        return Err("Cannot determine whether application exited".into());
    }
    let metadata = fs::metadata(&plan.app).map_err(|e| e.to_string())?;
    if metadata.dev() != plan.device || metadata.ino() != plan.inode {
        return Err("Another process replaced this application".into());
    }
    exchange(&plan.app, &new)?;
    let mut open = Command::new("/usr/bin/open");
    open.arg("-n");
    if let Some(root) = std::env::var_os("ECHO_DATA_DIR") {
        let mut variable = std::ffi::OsString::from("ECHO_DATA_DIR=");
        variable.push(root);
        open.arg("--env").arg(variable);
    }
    if let Err(error) = run(open.arg(&plan.app)) {
        exchange(&plan.app, &new).map_err(|rollback| format!(
            "Launch failed: {error}. Restoring previous app failed: {rollback}"))?;
        return Err(format!("Launch failed; previous app restored: {error}"));
    }
    fs::remove_dir_all(stage).map_err(|e| format!("Update installed; staging cleanup failed: {e}"))
}

pub fn helper(args: &[std::ffi::OsString]) -> Result<(), String> {
    if args.len() != 4 {
        return Err("Invalid update helper arguments".into());
    }
    let pid = args[3]
        .to_str()
        .ok_or("Invalid PID")?
        .parse()
        .map_err(|_| "Invalid PID")?;
    apply(Path::new(&args[2]), pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_creates_a_private_directory_on_the_install_volume() {
        let parent = tempfile::tempdir().unwrap();
        let stage = create_stage(parent.path()).unwrap();
        let metadata = fs::metadata(stage.path()).unwrap();
        assert_eq!(stage.path().parent(), Some(parent.path()));
        assert_eq!(metadata.uid(), unsafe { libc::getuid() });
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn atomic_exchange_handles_spaces_and_leaves_both_paths_untouched_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("Echo old.app");
        let b = dir.path().join("Echo 新.app");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(a.join("version"), "old").unwrap();
        fs::write(b.join("version"), "new").unwrap();
        exchange(&a, &b).unwrap();
        assert_eq!(fs::read_to_string(a.join("version")).unwrap(), "new");
        assert_eq!(fs::read_to_string(b.join("version")).unwrap(), "old");
        assert!(exchange(&a, &dir.path().join("missing")).is_err());
        assert_eq!(fs::read_to_string(a.join("version")).unwrap(), "new");
    }

    #[test]
    fn cancelling_prepared_installer_kills_helper_and_cleans_stage() {
        let stage = tempfile::tempdir().unwrap();
        let path = stage.path().to_owned();
        let child = Command::new("/bin/sh")
            .args(["-c", "read decision"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        drop(Installer {
            child,
            stage: Some(stage),
            committed: false,
        });
        assert!(!path.exists());
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    }
}
