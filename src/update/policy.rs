//! The signed minimum-version policy has a separate schedule from ordinary updates.
use super::{native, transport};
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Envelope {
    policy: String,
    signature: String,
}

#[derive(Deserialize)]
pub(super) struct Policy {
    pub issued: u64,
    min_version: Option<String>,
    platforms: BTreeMap<String, String>,
    pub message_zh: String,
    pub message_en: String,
}

impl Policy {
    fn verify(bytes: &[u8], key: &PublicKey) -> Result<Self, String> {
        let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let payload = envelope.policy.as_bytes();
        let signature = Signature::decode(&envelope.signature).map_err(|e| e.to_string())?;
        if signature.trusted_comment()
            != format!("echo-policy sha256:{:x}", Sha256::digest(payload))
        {
            return Err("Policy signature does not identify this document".into());
        }
        key.verify(payload, &signature, false)
            .map_err(|e| e.to_string())?;
        let policy: Self = serde_json::from_slice(payload).map_err(|e| e.to_string())?;
        for minimum in policy.min_version.iter().chain(policy.platforms.values()) {
            transport::version(minimum)?;
        }
        Ok(policy)
    }

    pub fn minimum(&self) -> Option<[u64; 3]> {
        let minimum = self
            .platforms
            .get(transport::platform().ok()?)
            .or(self.min_version.as_ref())?;
        let minimum = transport::version(minimum).ok()?;
        (minimum > transport::version(env!("CARGO_PKG_VERSION")).ok()?).then_some(minimum)
    }
}

fn refresh(sender: &mpsc::Sender<Policy>, wake: &rune::Waker) -> Result<(), String> {
    native::installed_app()?;
    let key = transport::public_key()?;
    let directory = crate::platform::data_root().join("updates");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let cache = directory.join("policy.json");
    let mut last = None;
    if cache.is_file() {
        let bytes = fs::read(&cache).map_err(|e| e.to_string())?;
        if let Ok(policy) = Policy::verify(&bytes, &key) {
            last = Some(policy.issued);
            sender.send(policy).map_err(|e| e.to_string())?;
            wake();
        }
    }
    let file = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    transport::fetch(
        &format!("{}/latest/policy.json", transport::DOWNLOAD_ORIGIN),
        file.path(),
        64 * 1024,
        20,
        &|_| {},
    )?;
    let bytes = fs::read(file.path()).map_err(|e| e.to_string())?;
    let policy = Policy::verify(&bytes, &key)?;
    if last.is_none_or(|issued| policy.issued > issued) {
        persist(file, &cache)?;
        sender.send(policy).map_err(|e| e.to_string())?;
        wake();
    }
    Ok(())
}

fn persist(mut file: tempfile::NamedTempFile, cache: &Path) -> Result<(), String> {
    file.flush()
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist(cache).map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) struct Monitor {
    pub current: Option<Policy>,
    pub retry: Instant,
    next: Instant,
    receiver: Option<mpsc::Receiver<Policy>>,
}

impl Monitor {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            current: None,
            retry: now,
            next: now + Duration::from_secs(3),
            receiver: None,
        }
    }

    pub fn poll(&mut self, wake: &rune::Waker, tx: &mut rune::Tx) -> bool {
        let now = Instant::now();
        let mut changed = false;
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(policy)
                        if self
                            .current
                            .as_ref()
                            .is_none_or(|old| old.issued < policy.issued) =>
                    {
                        self.current = Some(policy);
                        self.retry = now;
                        changed = true;
                    }
                    Ok(_) => {}
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.receiver = None;
                        break;
                    }
                }
            }
        }
        if now >= self.next && self.receiver.is_none() {
            self.next = now + Duration::from_secs(3600);
            let (sender, receiver) = mpsc::channel();
            let wake = wake.clone();
            if std::thread::Builder::new()
                .name("echo-update-policy".into())
                .spawn(move || {
                    if let Err(error) = refresh(&sender, &wake) {
                        eprintln!("Update policy: {error}");
                    }
                    drop(sender);
                    wake();
                })
                .is_ok()
            {
                self.receiver = Some(receiver);
            }
        }
        tx.wake_after(self.next.saturating_duration_since(now));
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEY: &str = "RWR/CYpEz0/mLu4SbkWo2dd94arcuUqVtbPH94y7aUqNLAThpGfSWu78";
    const POLICY: &[u8] = br#"{"policy": "{\"issued\":10,\"min_version\":\"999.0.0\",\"platforms\":{},\"message_en\":\"\",\"message_zh\":\"\"}", "signature": "untrusted comment: signature from minisign secret key\nRUR/CYpEz0/mLtY9Z7Nn4T4P695pT6Xse7GkhfXVQUp3TZQ515VTNsb+DYdnSj+YM4F0cbsBlLY6luJxCkfOf8zfs6q5myg05w0=\ntrusted comment: echo-policy sha256:77fa38bf323e13fc7731e5820ac0ee2a1c8728b3aef70d158e46267d2d8a9a94\niffkdBXFqst/hCkn8EJEez08zJh44eiqk270RB1RWEdk0XsCxZ/jbsBJMVVHLk2jn+mSluHByL3ukw36ypUWBw==\n"}"#;

    #[test]
    fn policy_requires_its_signed_payload_and_own_signature_domain() {
        let key = PublicKey::from_base64(KEY).unwrap();
        let policy = Policy::verify(POLICY, &key).unwrap();
        assert_eq!(policy.minimum(), Some([999, 0, 0]));
        let value = String::from_utf8(POLICY.to_vec()).unwrap();
        assert!(Policy::verify(value.replace("999.0.0", "998.0.0").as_bytes(), &key).is_err());
        assert!(
            Policy::verify(
                value
                    .replace("echo-policy sha256:", "package sha256:")
                    .as_bytes(),
                &key
            )
            .is_err()
        );
        let mut policy = policy;
        policy
            .platforms
            .insert(transport::platform().unwrap().into(), "0.0.0".into());
        assert_eq!(policy.minimum(), None);
    }
}
