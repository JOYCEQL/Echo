//! Application updates have their own worker and state; they never access document data.
mod control;
pub use control::{policy_notice, update_button};
mod installer;
mod policy;
mod proxy;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod native;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod native;
mod transport;

use crate::model::State;
use native::Prepared;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use transport::Manifest;

pub enum Status {
    Unavailable,
    Idle,
    Checking,
    Current,
    Available(Manifest),
    Downloading(u8),
    Verifying,
    Ready(Prepared),
    Preparing,
    Installing,
    CheckFailed(String),
    Failed(String),
}
impl Status {
    pub fn visible_in_sidebar(&self) -> bool {
        matches!(self, Self::Available(_) | Self::Downloading(_) | Self::Verifying
            | Self::Ready(_) | Self::Preparing | Self::Installing | Self::Failed(_))
    }

    fn failure(&self, error: String) -> Self {
        match self {
            Self::Checking => Self::CheckFailed(error),
            _ => Self::Failed(error),
        }
    }
}
enum WorkerEvent {
    Progress(u8),
    Finished(Result<Answer, String>),
}
enum Answer {
    Checked(Option<Manifest>),
    Downloaded(Prepared),
    Installer(native::Installer),
}

pub struct Updater {
    pub status: Status,
    pub automatic: bool,
    worker: Option<mpsc::Receiver<WorkerEvent>>,
    next_check: Instant,
    last_check: Option<Instant>,
    policy: policy::Monitor,
    installer: Option<native::Installer>,
}
impl Updater {
    pub fn new(automatic: bool) -> Self {
        Self {
            status: if transport::public_key().is_ok() {
                Status::Idle
            } else {
                Status::Unavailable
            },
            automatic,
            worker: None,
            next_check: Instant::now() + Duration::from_secs(10),
            installer: None,
            last_check: None,
            policy: policy::Monitor::new(),
        }
    }
    pub fn foreground(&mut self) {
        if self.automatic && self.last_check.is_some_and(|last| last.elapsed() >= Duration::from_secs(3600)) {
            self.next_check = Instant::now();
        }
    }

    fn required(&self) -> Option<[u64; 3]> {
        self.policy.current.as_ref()?.minimum()
    }

    pub fn required_ready(&self) -> bool {
        matches!((&self.status, self.required()), (Status::Ready(p), Some(minimum))
            if transport::version(&p.manifest.version).is_ok_and(|version| version >= minimum))
    }

    fn start(
        &mut self,
        wake: rune::Waker,
        job: impl FnOnce(&dyn Fn(u8)) -> Result<Answer, String> + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("echo-updater".into())
            .spawn(move || {
                let result = job(&|percent| {
                    if sender.send(WorkerEvent::Progress(percent)).is_ok() {
                        wake();
                    }
                });
                let _ = sender.send(WorkerEvent::Finished(result));
                wake();
            }) {
            Ok(_) => self.worker = Some(receiver),
            Err(error) => self.status = self.status.failure(error.to_string()),
        }
    }
    pub fn check(&mut self, wake: rune::Waker) {
        if self.worker.is_some()
            || matches!(
                self.status,
                Status::Unavailable | Status::Ready(_) | Status::Preparing | Status::Installing
            )
        {
            return;
        }
        self.last_check = Some(Instant::now());
        self.next_check = Instant::now() + Duration::from_secs(12 * 3600);
        self.status = Status::Checking;
        self.start(wake, |_| transport::check().map(Answer::Checked));
    }
    pub fn download(&mut self, wake: rune::Waker) {
        let Status::Available(manifest) = &self.status else {
            return;
        };
        let manifest = manifest.clone();
        self.status = Status::Downloading(0);
        self.start(wake, move |progress| {
            native::prepare(manifest, progress).map(Answer::Downloaded)
        });
    }
    fn poll(&mut self) -> bool {
        let Some(receiver) = &self.worker else {
            return false;
        };
        let mut changed = false;
        let result = loop {
            match receiver.try_recv() {
                Ok(WorkerEvent::Progress(percent)) => {
                    self.status = if percent == 100 {
                        Status::Verifying
                    } else {
                        Status::Downloading(percent)
                    };
                    changed = true;
                }
                Ok(WorkerEvent::Finished(result)) => break result,
                Err(mpsc::TryRecvError::Empty) => return changed,
                Err(mpsc::TryRecvError::Disconnected) => break Err("Update worker stopped".into()),
            }
        };
        self.worker = None;
        self.status = match result {
            Ok(Answer::Checked(Some(manifest))) => Status::Available(manifest),
            Ok(Answer::Checked(None)) => Status::Current,
            Ok(Answer::Downloaded(prepared)) => Status::Ready(prepared),
            Ok(Answer::Installer(installer)) => {
                self.installer = Some(installer);
                Status::Preparing
            }
            Err(error) => self.status.failure(error),
        };
        true
    }
}

pub fn poll(s: &mut State, tx: &mut rune::Tx) -> bool {
    let mut changed = s.updater.poll();
    if let Some(installer) = s.updater.installer.take() {
        match save_before_install(s).and_then(|()| installer.commit()) {
            Ok(()) => {
                s.updater.status = Status::Installing;
                tx.quit();
            }
            Err(error) => s.updater.status = Status::Failed(error),
        }
        changed = true;
    }
    if !matches!(s.updater.status, Status::Unavailable)
        && let Some(wake) = s.window_waker.clone() {
        changed |= s.updater.policy.poll(&wake, tx);
        if let Some(minimum) = s.updater.required() {
            let now = Instant::now();
            if s.updater.worker.is_none() {
                match &s.updater.status {
                    Status::Available(m) if transport::version(&m.version).is_ok_and(|v| v >= minimum) => {
                        s.updater.download(wake);
                        s.updater.policy.retry = now + Duration::from_secs(600);
                        changed = true;
                    }
                    Status::Ready(_) if !s.updater.required_ready() => {
                        s.updater.status = Status::Idle;
                        s.updater.policy.retry = now;
                        changed = true;
                        tx.wake_after(Duration::ZERO);
                    }
                    Status::Ready(_) | Status::Preparing | Status::Installing => {}
                    _ if now >= s.updater.policy.retry => {
                        s.updater.policy.retry = now + Duration::from_secs(600);
                        s.updater.check(wake);
                        changed = true;
                    }
                    _ => tx.wake_after(s.updater.policy.retry - now),
                }
            }
            return changed;
        }
    }
    if s.updater.automatic
        && !matches!(
            s.updater.status,
            Status::Unavailable | Status::Ready(_) | Status::Preparing | Status::Installing
        )
    {
        let now = Instant::now();
        if now >= s.updater.next_check && s.updater.worker.is_none() {
            if let Some(wake) = &s.window_waker {
                s.updater.check(wake.clone());
                changed = true;
            }
        }
        if s.updater.next_check > now {
            tx.wake_after(s.updater.next_check - now);
        }
    }
    changed
}

pub fn act(s: &mut State, tx: &mut rune::Tx) {
    if matches!(s.updater.status, Status::Ready(_)) {
        let Some(wake) = s.window_waker.clone() else {
            return;
        };
        let Status::Ready(prepared) = std::mem::replace(&mut s.updater.status, Status::Preparing)
        else {
            unreachable!()
        };
        s.updater
            .start(wake, move |_| prepared.launch().map(Answer::Installer));
    } else if let Some(wake) = &s.window_waker {
        if matches!(s.updater.status, Status::Available(_)) && s.updater.required().is_none() {
            s.updater.download(wake.clone());
        } else {
            s.updater.check(wake.clone());
        }
    }
    tx.instant();
}

fn save_before_install(s: &mut State) -> Result<(), String> {
    s.finish_edit();
    if s.document_save_failed {
        return Err("Update cancelled: document could not be saved".into());
    }
    s.save();
    if s.document_save_failed {
        return Err("Update cancelled: library could not be saved".into());
    }
    s.store.flush_background()
}

pub fn release_note(s: &State) -> Option<&'static str> {
    match (env!("CARGO_PKG_VERSION"), s.language) {
        ("0.0.1", crate::i18n::Language::Chinese) => Some("安装包和更新现已通过 Echo 下载域名分发。Mac 支持 Apple Silicon 和 Intel。"),
        ("0.0.1", crate::i18n::Language::English) => Some("Echo now delivers installers and updates from its own download domain. Mac downloads are available for Apple Silicon and Intel."),
        _ => None,
    }
}

pub fn show_release_note(s: &mut State) {
    let current = env!("CARGO_PKG_VERSION");
    let previous = s.store.preference("last_started_version");
    if previous.as_deref() == Some(current) { return; }
    if let Err(error) = s.store.set_preference("last_started_version", current) {
        eprintln!("Could not record app version: {error}");
        return;
    }
    if previous.as_deref().and_then(|value| transport::version(value).ok())
        .is_some_and(|old| transport::version(current).is_ok_and(|now| old < now))
        && s.message.is_none() && let Some(note) = release_note(s) {
        s.message = Some(format!("Echo {current} · {note}"));
    }
}

pub fn toggle(s: &mut State, tx: &mut rune::Tx) {
    let enabled = !s.updater.automatic;
    match s
        .store
        .set_preference("automatic_updates", if enabled { "true" } else { "false" })
    {
        Ok(()) => {
            s.updater.automatic = enabled;
            s.updater.next_check = Instant::now();
        }
        Err(error) => s.message = Some(error),
    }
    tx.instant();
}

pub fn helper(args: &[std::ffi::OsString]) -> Result<(), String> {
    native::helper(args)
}

#[cfg(test)]
mod tests;
