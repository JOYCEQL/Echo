//! OS integration; each supported platform owns its paths, window and process semantics.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "windows")]
pub use windows::*;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Echo currently supports macOS and Windows");

#[cfg(test)]
mod tests {
    #[test]
    fn atomic_replacement_creates_and_overwrites_unicode_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文 笔记.md");
        for text in ["initial", "updated"] {
            let temp = dir.path().join("staging");
            std::fs::write(&temp, text).unwrap();
            std::fs::OpenOptions::new()
                .write(true)
                .open(&temp)
                .unwrap()
                .sync_all()
                .unwrap();
            super::replace_file(&temp, &path).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
            assert!(!temp.exists());
        }
    }
}
