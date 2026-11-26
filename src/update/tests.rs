use super::*;
use transport::{Manifest, version};

fn manifest() -> Manifest {
    Manifest {
        version: "0.11.0".into(),
        platform: "macos-arm64".into(),
        url: "https://echo-downloads.magicv.art/v0.11.0/Echo-0.11.0-macos-arm64.zip".into(),
        size: include_bytes!("fixtures/package.txt").len() as u64,
        signature: include_str!("fixtures/package.minisig").into(),
    }
}

#[test]
fn versions_are_numeric_and_reject_ambiguous_or_unsafe_values() {
    assert!(version("0.11.0").unwrap() > version("0.9.9").unwrap());
    assert_eq!(version("0.10.4").unwrap(), [0, 10, 4]);
    for value in [
        "",
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.2.3-beta",
        "v1.2.3",
        "1.2.3/../",
        "1.2.-3",
        "1.2.9999999999999999999999999",
    ] {
        assert!(version(value).is_err(), "{value}");
    }
}

#[test]
fn signed_package_rejects_corruption_truncation_wrong_key_and_relabelled_versions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("package.zip");
    let bytes = include_bytes!("fixtures/package.txt");
    std::fs::write(&path, bytes).unwrap();
    let key =
        minisign_verify::PublicKey::from_base64(include_str!("fixtures/public-key.txt").trim())
            .unwrap();
    let m = manifest();
    m.validate("macos-arm64").unwrap();
    m.verify(&path, &key).unwrap();
    let mut tampered = bytes.to_vec();
    tampered[0] ^= 1;
    std::fs::write(&path, &tampered).unwrap();
    assert!(m.verify(&path, &key).is_err());
    std::fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
    assert!(m.verify(&path, &key).is_err());
    std::fs::write(&path, bytes).unwrap();
    let mut changed_key = include_str!("fixtures/public-key.txt").trim().to_owned();
    changed_key.replace_range(
        20..21,
        if &changed_key[20..21] == "A" {
            "B"
        } else {
            "A"
        },
    );
    let wrong_key = minisign_verify::PublicKey::from_base64(&changed_key).unwrap();
    assert!(m.verify(&path, &wrong_key).is_err());
    let mut replay = m.clone();
    replay.version = "9.0.0".into();
    assert!(replay.validate("macos-arm64").is_err());
    replay.url = replay.expected_url();
    replay.signature = replay.signature.replace("0.11.0", "9.0.0");
    replay.validate("macos-arm64").unwrap();
    assert!(
        replay.verify(&path, &key).is_err(),
        "rewriting the signed comment must fail verification"
    );
}

#[test]
fn manifests_cannot_redirect_downloads_or_choose_another_platform() {
    let mut m = manifest();
    assert_eq!(
        m.expected_url(),
        "https://echo-downloads.magicv.art/v0.11.0/Echo-0.11.0-macos-arm64.zip"
    );
    assert!(m.validate("macos-x86_64").is_err());
    for size in [0, 512 * 1024 * 1024 + 1] {
        m.size = size;
        assert!(m.validate("macos-arm64").is_err());
    }
    let mut json = serde_json::to_value(manifest()).unwrap();
    json["url"] = "https://attacker.example/file.zip".into();
    assert!(serde_json::from_value::<Manifest>(json).unwrap().validate("macos-arm64").is_err());
}

#[test]
fn background_completion_and_disconnection_leave_no_busy_worker() {
    let mut updater = Updater::new(false);
    let (sender, receiver) = mpsc::channel();
    updater.status = Status::Checking;
    updater.worker = Some(receiver);
    assert!(!updater.poll());
    sender
        .send(WorkerEvent::Finished(Ok(Answer::Checked(Some(manifest())))))
        .ok()
        .unwrap();
    assert!(updater.poll());
    assert!(matches!(updater.status, Status::Available(_)));
    assert!(updater.status.visible_in_sidebar());
    assert!(updater.worker.is_none());
    let (sender, receiver) = mpsc::channel();
    updater.worker = Some(receiver);
    sender.send(WorkerEvent::Progress(47)).ok().unwrap();
    assert!(updater.poll());
    assert!(matches!(updater.status, Status::Downloading(47)));
    sender.send(WorkerEvent::Progress(100)).ok().unwrap();
    assert!(updater.poll());
    assert!(matches!(updater.status, Status::Verifying));
    assert!(updater.worker.is_some());
    drop(sender);
    assert!(updater.poll());
    assert!(matches!(updater.status, Status::Failed(_)));
    assert!(updater.worker.is_none());
    let (sender, receiver) = mpsc::channel();
    updater.status = Status::Checking;
    updater.worker = Some(receiver);
    drop(sender);
    assert!(updater.poll());
    assert!(matches!(updater.status, Status::CheckFailed(_)));
    assert!(!updater.status.visible_in_sidebar());
}

#[test]
fn save_barrier_flushes_background_writes_and_rejects_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::from_root(dir.path().to_owned());
    s.choose_mode(crate::model::WorkspaceMode::Document);
    s.create_doc_note("工作", None);
    s.save();
    let current = s.current.clone();
    s.editor.active.core.insert_text("更新前输入的文字");
    crate::doc_editor::sync_to_state(&mut s);
    s.queue_document_save();
    assert!(s.start_document_save());
    save_before_install(&mut s).unwrap();
    assert!(!s.store.saving());
    assert!(s.pending_document_save.is_none());
    assert_eq!(
        s.store.load_note(&current).unwrap().doc,
        s.note().unwrap().doc
    );
    // Replace the library's index externally: updating must not quit over a save conflict.
    let path = dir.path().join(".echo/index.json");
    let old = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"broken external index").unwrap();
    assert!(save_before_install(&mut s).is_err());
    assert!(s.document_save_failed);
    std::fs::write(&path, old).unwrap();
    save_before_install(&mut s).unwrap();
}

#[test]
fn windows_packages_use_setup_names_and_cannot_reuse_a_mac_signature() {
    let mut m = manifest();
    m.platform = "windows-x64".into();
    assert_eq!(m.name(), "Echo-0.11.0-windows-x64-setup.exe");
    assert_eq!(
        m.expected_url(),
        "https://echo-downloads.magicv.art/v0.11.0/Echo-0.11.0-windows-x64-setup.exe"
    );
    assert!(m.validate("windows-x64").is_err());
}
