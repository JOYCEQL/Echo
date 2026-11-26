//! Release protocol. No UI, library access, runtime endpoint overrides or unsigned mode.
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, Write},
    path::Path,
    process::Stdio,
};

const MAX_PACKAGE: u64 = 512 * 1024 * 1024;

pub const DOWNLOAD_ORIGIN: &str = "https://echo-downloads.magicv.art";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: String,
    pub platform: String,
    pub size: u64,
    pub url: String,
    pub signature: String,
}

pub fn platform() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("macos-arm64"),
        ("macos", "x86_64") => Ok("macos-x86_64"),
        ("windows", "x86_64") => Ok("windows-x64"),
        _ => Err("Unsupported update platform".into()),
    }
}

pub fn public_key() -> Result<PublicKey, String> {
    PublicKey::decode(include_str!("../../packaging/echo-update.pub"))
        .map_err(|e| format!("Invalid release update key: {e}"))
}

pub fn version(value: &str) -> Result<[u64; 3], String> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3 {
        return Err("Expected a stable major.minor.patch version".into());
    }
    let mut numbers = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return Err("Invalid release version".into());
        }
        numbers[i] = part.parse().map_err(|_| "Release version overflow")?;
    }
    Ok(numbers)
}

impl Manifest {
    pub fn name(&self) -> String {
        let suffix = if self.platform == "windows-x64" {
            "-setup.exe"
        } else {
            ".zip"
        };
        format!("Echo-{}-{}{suffix}", self.version, self.platform)
    }
    pub fn expected_url(&self) -> String {
        format!("{DOWNLOAD_ORIGIN}/v{}/{}", self.version, self.name())
    }
    pub fn validate(&self, expected_platform: &str) -> Result<(), String> {
        version(&self.version)?;
        if self.platform != expected_platform {
            return Err("Update platform mismatch".into());
        }
        if self.url != self.expected_url() {
            return Err("Unexpected update package URL".into());
        }
        if self.size == 0 || self.size > MAX_PACKAGE {
            return Err("Invalid update size".into());
        }
        let signature = Signature::decode(&self.signature).map_err(|e| e.to_string())?;
        if signature.trusted_comment() != self.name() {
            return Err("Update signature names another release".into());
        }
        Ok(())
    }
    pub fn verify(&self, path: &Path, key: &PublicKey) -> Result<(), String> {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() != self.size {
            return Err("Update size mismatch".into());
        }
        let signature = Signature::decode(&self.signature).map_err(|e| e.to_string())?;
        if signature.trusted_comment() != self.name() {
            return Err("Update signature names another release".into());
        }
        let mut verifier = key.verify_stream(&signature).map_err(|e| e.to_string())?;
        let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut buffer = [0_u8; 64 * 1024];
        let mut total = 0_u64;
        loop {
            let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > self.size {
                return Err("Update grew during verification".into());
            }
            verifier.update(&buffer[..read]);
        }
        if total != self.size {
            return Err("Update changed during verification".into());
        }
        verifier
            .finalize()
            .map_err(|e| format!("Update signature rejected: {e}"))
    }
}

#[cfg(target_os = "macos")]
pub fn run(command: &mut std::process::Command) -> Result<(), String> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn copy_download(
    mut source: impl Read,
    mut out: impl Write,
    limit: u64,
    progress: &dyn Fn(u8),
) -> Result<u64, String> {
    let mut buffer = [0_u8; 64 * 1024];
    let mut received = 0;
    let mut last_percent = 0;
    loop {
        let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(received);
        }
        received += count as u64;
        if received > limit {
            return Err("Download exceeds size limit".into());
        }
        out.write_all(&buffer[..count]).map_err(|e| e.to_string())?;
        // Reserve 100 for a successful transfer with the exact manifest size.
        let percent = (received * 100 / limit).min(99) as u8;
        if percent != last_percent {
            progress(percent);
            last_percent = percent;
        }
    }
}

pub(super) fn fetch(
    url: &str,
    destination: &Path,
    max_size: u64,
    timeout: u32,
    progress: &dyn Fn(u8),
) -> Result<u64, String> {
    let out = fs::File::create(destination).map_err(|e| e.to_string())?;
    let mut errors = tempfile::tempfile().map_err(|e| e.to_string())?;
    let error_sink = errors.try_clone().map_err(|e| e.to_string())?;
    let mut command = crate::platform::curl()?;
    command.arg("--disable");
    super::proxy::configure(&mut command);
    let mut child = command
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
            &timeout.to_string(),
            "--max-filesize",
            &max_size.to_string(),
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(error_sink))
        .spawn()
        .map_err(|e| format!("Download could not start: {e}"))?;
    let pipe = child.stdout.take().expect("curl stdout is piped");
    let copied = copy_download(pipe, out, max_size, progress);
    if copied.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let size = copied?;
    if !status.success() {
        errors.rewind().map_err(|e| e.to_string())?;
        let mut message = String::new();
        errors
            .take(4096)
            .read_to_string(&mut message)
            .map_err(|e| e.to_string())?;
        return Err(format!("Download failed ({status}): {}", message.trim()));
    }
    Ok(size)
}

pub fn check() -> Result<Option<Manifest>, String> {
    public_key()?;
    super::native::installed_app()?;
    let platform = platform()?;
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = directory.path().join("manifest.json");
    fetch(
        &format!("{DOWNLOAD_ORIGIN}/latest/update-{platform}.json"),
        &file,
        64 * 1024,
        30,
        &|_| {},
    )?;
    let manifest: Manifest = serde_json::from_slice(&fs::read(&file).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Invalid update manifest: {e}"))?;
    manifest.validate(platform)?;
    Ok((version(&manifest.version)? > version(env!("CARGO_PKG_VERSION"))?).then_some(manifest))
}

pub fn download(
    manifest: &Manifest,
    destination: &Path,
    progress: &dyn Fn(u8),
) -> Result<(), String> {
    manifest.validate(platform()?)?;
    let key = public_key()?;
    let url = &manifest.url;
    let transfer = |request: &str| -> Result<Result<(), String>, String> {
        let received = fetch(request, destination, manifest.size, 900, progress)?;
        if received != manifest.size {
            return Ok(Err("Update size mismatch".into()));
        }
        progress(100);
        Ok(manifest.verify(destination, &key))
    };
    if transfer(url)?.is_ok() {
        return Ok(());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    progress(0);
    transfer(&format!("{url}?nocache={nonce}"))?
}

#[cfg(test)]
mod transfer_tests {
    use super::copy_download;
    use std::cell::RefCell;

    #[test]
    fn reports_written_bytes_and_never_marks_an_incomplete_transfer_verified() {
        let chunk = 64 * 1024;
        let package = vec![42; 4 * chunk];
        let progress = RefCell::new(Vec::new());
        let report = |n| progress.borrow_mut().push(n);
        let mut output = Vec::new();
        let size = package.len() as u64;
        assert_eq!(
            copy_download(&package[..], &mut output, size, &report).unwrap(),
            size
        );
        assert_eq!(*progress.borrow(), [25, 50, 75, 99]);
        assert_eq!(output, package);
        progress.borrow_mut().clear();
        assert_eq!(
            copy_download(&package[..chunk], std::io::sink(), size, &report).unwrap(),
            chunk as u64
        );
        assert_eq!(*progress.borrow(), [25]);
        assert!(copy_download(&package[..], std::io::sink(), size - 1, &report).is_err());
    }
}
