use std::{
    ffi::OsStr,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};
use windows_sys::Win32::{
    Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
    UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

pub const UI_FONT: &str = "Segoe UI, Microsoft YaHei UI";
pub const CODE_FONT: &str = "Consolas";
pub const CHROME_LEADING: f32 = 16.;
pub fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
pub fn data_root() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA must be set")).join("Echo")
}
pub fn open_folder(path: &Path) -> Result<(), String> {
    let path = wide(path.as_os_str());
    let verb = wide(OsStr::new("open"));
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err(format!(
            "Explorer could not open the folder (ShellExecute {result})"
        ))
    }
}
pub fn curl() -> Result<Command, String> {
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot must be set")?;
    let mut command = Command::new(PathBuf::from(root).join("System32/curl.exe"));
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    Ok(command)
}
pub fn chrome() -> rune::Chrome {
    rune::Chrome { drag_regions: true, ..rune::Chrome::DEFAULT }
}
pub fn window_options() -> rune::WindowOptions {
    rune::WindowOptions { transparent: true, ..Default::default() }
}
pub fn backdrop(s: &crate::model::State, element: rune::El) -> rune::El {
    let stops = s.sidebar_material.surface_stops(s.system_dark.get());
    element.linear_gradient(std::f32::consts::FRAC_PI_2, &stops)
}
pub fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let from = wide(from.as_os_str());
    let to = wide(to.as_os_str());
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub const FONT_SMOOTHING: rune::FontSmoothing = rune::FontSmoothing::DirectWrite;
