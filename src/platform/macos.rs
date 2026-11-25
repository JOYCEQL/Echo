use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub const UI_FONT: &str = "System Font, PingFang SC";
pub const CODE_FONT: &str = "Menlo";
pub const CHROME_LEADING: f32 = 92.;

pub fn data_root() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME must be set"))
        .join("Library/Application Support/Echo")
}
pub fn open_folder(path: &Path) -> Result<(), String> {
    Command::new("/usr/bin/open")
        .arg(path)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}
pub fn curl() -> Result<Command, String> {
    Ok(Command::new("/usr/bin/curl"))
}
pub fn chrome() -> rune::Chrome {
    rune::Chrome {
        full_size_content: true,
        traffic_lights: Some((14., crate::ui::TRAFFIC_LIGHT_Y)),
        drag_regions: true,
    }
}
pub fn window_options() -> rune::WindowOptions {
    rune::WindowOptions {
        transparent: true,
        ..Default::default()
    }
}
pub fn backdrop(s: &crate::model::State, element: rune::El) -> rune::El {
    element
        .fill(rune::Color::TRANSPARENT)
        .system_material(s.sidebar_material.native_backdrop(s.system_dark.get()))
        .clip_rounded(18.)
}
pub fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)?;
    std::fs::File::open(to.parent().expect("file has a parent"))?.sync_all()
}

pub const FONT_SMOOTHING: rune::FontSmoothing = rune::FontSmoothing::Off;
