use std::{
    io::{BufRead, Write},
    process::Child,
    time::Duration,
};
pub struct Installer {
    pub(super) child: Child,
    pub(super) stage: Option<tempfile::TempDir>,
    pub(super) committed: bool,
}
impl Installer {
    pub fn commit(mut self) -> Result<(), String> {
        self.child
            .stdin
            .take()
            .ok_or("Installer control channel missing")?
            .write_all(b"INSTALL\n")
            .map_err(|e| e.to_string())?;
        self.committed = true;
        if let Some(stage) = self.stage.take() {
            let _ = stage.keep();
        }
        Ok(())
    }
}
impl Drop for Installer {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Installer {
    pub fn ready(child: Child, stage: tempfile::TempDir) -> Result<Self, String> {
        let mut installer = Self {
            child,
            stage: Some(stage),
            committed: false,
        };
        let stdout = installer
            .child
            .stdout
            .take()
            .ok_or("Installer handshake channel missing")?;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("echo-installer-handshake".into())
            .spawn(move || {
                let mut line = String::new();
                let result = std::io::BufReader::new(stdout)
                    .read_line(&mut line)
                    .map(|_| line);
                let _ = sender.send(result);
            })
            .map_err(|e| e.to_string())?;
        let answer = receiver
            .recv_timeout(Duration::from_secs(45))
            .map_err(|e| format!("Installer did not become ready: {e}"))?
            .map_err(|e| e.to_string())?;
        if answer != "READY\n" {
            let details = std::fs::read_to_string(
                installer
                    .stage
                    .as_ref()
                    .expect("stage retained")
                    .path()
                    .join("install.log"),
            )
            .map_err(|e| format!("Installer rejected update; cannot read log: {e}"))?;
            return Err(format!("Installer rejected update: {}", details.trim()));
        }
        Ok(installer)
    }
}
