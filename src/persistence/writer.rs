//! A durable serial executor: accepted jobs run to completion, including during shutdown.
//! Unlike a latest-result request runner, saving never supersedes an earlier write.
use std::{
    cell::RefCell,
    sync::{Arc, mpsc},
    thread,
};

type Job = Box<dyn FnOnce() + Send>;
type Wake = Arc<dyn Fn() + Send + Sync>;
struct Running {
    sender: mpsc::Sender<Job>,
    thread: thread::JoinHandle<()>,
}

#[derive(Default)]
pub struct Writer {
    running: RefCell<Option<Running>>,
    wake: RefCell<Option<Wake>>,
}

impl Writer {
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self.wake.borrow_mut() = Some(Arc::new(wake));
    }

    pub fn submit(&self, job: impl FnOnce() + Send + 'static) -> Result<(), String> {
        let mut running = self.running.borrow_mut();
        if running.is_none() {
            let (sender, receiver) = mpsc::channel::<Job>();
            let thread = thread::Builder::new()
                .name("echo-library-writer".into())
                .spawn(move || {
                    for job in receiver {
                        job();
                    }
                })
                .map_err(|error| format!("Could not start library writer: {error}"))?;
            *running = Some(Running { sender, thread });
        }
        let wake = self.wake.borrow().clone();
        running
            .as_ref()
            .expect("writer started")
            .sender
            .send(Box::new(move || {
                job();
                if let Some(wake) = wake {
                    wake();
                }
            }))
            .map_err(|_| "Library writer stopped".into())
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if let Some(Running { sender, thread }) = self.running.get_mut().take() {
            drop(sender);
            if thread.join().is_err() {
                eprintln!("Echo library writer panicked");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_drains_every_accepted_write_in_order_and_wakes_after_completion() {
        use std::sync::Mutex;
        let values = Arc::new(Mutex::new(Vec::new()));
        let wakes = Arc::new(Mutex::new(Vec::new()));
        let writer = Writer::default();
        let (written, notified) = (values.clone(), wakes.clone());
        writer.set_waker(move || notified.lock().unwrap().push(written.lock().unwrap().len()));
        for i in 0..64 {
            let values = values.clone();
            writer
                .submit(move || values.lock().unwrap().push(i))
                .unwrap();
        }
        drop(writer);
        assert_eq!(*values.lock().unwrap(), (0..64).collect::<Vec<_>>());
        assert_eq!(*wakes.lock().unwrap(), (1..=64).collect::<Vec<_>>());
    }
}
