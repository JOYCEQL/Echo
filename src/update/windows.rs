//! Inno Setup replaces a per-user installation after the saving process exits.
pub use super::installer::Installer;
use super::transport::{self, Manifest};
use std::{
    fs,
    io::Write,
    os::windows::{ffi::OsStringExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    System::Threading::{
        CREATE_NO_WINDOW, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        QueryFullProcessImageNameW, WaitForSingleObject,
    },
};

pub struct Prepared {
    pub manifest: Manifest,
    stage: tempfile::TempDir,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    manifest: Manifest,
    app: PathBuf,
}

fn validate_install(app: &Path) -> Result<(), String> {
    if app.file_name().is_none_or(|name| name != "Echo.exe")
        || !app.is_file()
        || !app
            .parent()
            .ok_or("Missing install directory")?
            .join("unins000.exe")
            .is_file()
    {
        return Err("Updates require Echo installed with the Windows setup package".into());
    }
    Ok(())
}
pub fn installed_app() -> Result<PathBuf, String> {
    let app = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    validate_install(&app)?;
    Ok(app)
}
pub fn prepare(manifest: Manifest, progress: &dyn Fn(u8)) -> Result<Prepared, String> {
    let app = installed_app()?;
    let stage = tempfile::Builder::new()
        .prefix("echo-update-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    transport::download(&manifest, &stage.path().join("setup.exe"), progress)?;
    fs::copy(&app, stage.path().join("helper.exe")).map_err(|e| e.to_string())?;
    fs::write(
        stage.path().join("plan.json"),
        serde_json::to_vec(&Plan {
            manifest: manifest.clone(),
            app,
        })
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(Prepared { manifest, stage })
}
impl Prepared {
    pub fn launch(self) -> Result<Installer, String> {
        let log =
            fs::File::create(self.stage.path().join("install.log")).map_err(|e| e.to_string())?;
        // Run outside the installation directory: Windows locks executing PE files.
        let child = Command::new(self.stage.path().join("helper.exe"))
            .arg("--apply-update")
            .arg(self.stage.path())
            .arg(std::process::id().to_string())
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| e.to_string())?;
        Installer::ready(child, self.stage)
    }
}
struct Process(windows_sys::Win32::Foundation::HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub fn helper(args: &[std::ffi::OsString]) -> Result<(), String> {
    if args.len() != 4 {
        return Err("Invalid update helper arguments".into());
    }
    let pid: u32 = args[3]
        .to_str()
        .ok_or("Invalid PID")?
        .parse()
        .map_err(|_| "Invalid PID")?;
    if pid == 0 || pid == std::process::id() {
        return Err("Invalid parent process".into());
    }
    let stage = Path::new(&args[2])
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let own = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if own != stage.join("helper.exe") {
        return Err("Helper staging path mismatch".into());
    }
    let plan: Plan =
        serde_json::from_slice(&fs::read(stage.join("plan.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    validate_install(&plan.app)?;
    plan.manifest.validate(transport::platform()?)?;
    if transport::version(&plan.manifest.version)? <= transport::version(env!("CARGO_PKG_VERSION"))?
    {
        return Err("Update is not newer".into());
    }
    plan.manifest
        .verify(&stage.join("setup.exe"), &transport::public_key()?)?;
    let handle = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let parent = Process(handle);
    let mut image = vec![0_u16; 32768];
    let mut length = image.len() as u32;
    if unsafe { QueryFullProcessImageNameW(parent.0, 0, image.as_mut_ptr(), &mut length) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let parent_path = PathBuf::from(std::ffi::OsString::from_wide(&image[..length as usize]))
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if parent_path != plan.app {
        return Err("Update parent does not own target installation".into());
    }
    println!("READY");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut decision = String::new();
    std::io::stdin()
        .read_line(&mut decision)
        .map_err(|e| e.to_string())?;
    if decision != "INSTALL\n" {
        return Err("Installation was not committed".into());
    }
    if unsafe { WaitForSingleObject(parent.0, 120_000) } != WAIT_OBJECT_0 {
        return Err("Application did not exit; update cancelled".into());
    }
    // Canonical Windows paths carry a verbatim prefix; Inno expects a DOS path.
    let directory = plan.app.parent().ok_or("Missing install directory")?;
    let target = directory.to_str().ok_or("Install path is not Unicode")?;
    let target = target
        .strip_prefix(r"\\?\")
        .ok_or("Expected canonical Windows install path")?;
    if target.starts_with("UNC\\") {
        return Err("Network installations are not supported".into());
    }
    let status = Command::new(stage.join("setup.exe"))
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/NOCANCEL",
            "/UPDATE=1",
        ])
        .arg(format!("/DIR={target}"))
        .arg(format!("/LOG={}", stage.join("setup.log").display()))
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "Windows setup failed ({status}); logs: {}",
            stage.display()
        ));
    }
    Command::new(&plan.app)
        .spawn()
        .map_err(|e| format!("Update installed but could not restart: {e}"))?;
    // The running helper cannot delete its own executable. Keep its small diagnostics
    // directory in TEMP; remove the downloaded installer after a successful install.
    fs::remove_file(stage.join("setup.exe")).map_err(|e| e.to_string())?;
    Ok(())
}
