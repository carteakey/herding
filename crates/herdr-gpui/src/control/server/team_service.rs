//! GPUI-facing bridge over the bounded team worker.
//!
//! Submission queues work. `poll` drains ordered completions from a bounded
//! mailbox and never performs database I/O.

use super::team_worker::{Error, Operation, Reply, Worker};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex, TryLockError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
};

const COMPLETIONS: usize = 16;

pub(crate) struct Completion {
    pub request_id: u64,
    pub result: Result<Reply, Arc<Error>>,
}

type Watched = (u64, Receiver<Result<Reply, Error>>);
type Mailbox = Arc<(Mutex<VecDeque<Completion>>, Condvar)>;

pub(crate) struct Service {
    worker: Option<Worker>,
    completed: Option<SyncSender<Watched>>,
    mailbox: Mailbox,
    next_request_id: u64,
    stopping: Arc<AtomicBool>,
    #[cfg(test)]
    collector: Option<thread::JoinHandle<()>>,
}

impl Service {
    /// Uses `<state_dir>/team/team.sqlite3`. This resolves a path and spawns
    /// threads only; the SQLite connection opens on the store worker.
    pub(crate) fn start() -> std::io::Result<Self> {
        let state = herdr_client::local_path::state_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No app state directory is available",
            )
        })?;
        Self::start_at(state.join("team").join("team.sqlite3"))
    }

    fn start_at(path: PathBuf) -> std::io::Result<Self> {
        let worker = Worker::start(path)?;
        let (completed, incoming) = mpsc::sync_channel(COMPLETIONS);
        let mailbox = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let stopping = Arc::new(AtomicBool::new(false));
        let collector_mailbox = Arc::clone(&mailbox);
        let collector_stopping = Arc::clone(&stopping);
        let _collector = thread::Builder::new()
            .name("herdr-team-completions".into())
            .spawn(move || collect(incoming, collector_mailbox, collector_stopping))?;
        Ok(Self {
            worker: Some(worker),
            completed: Some(completed),
            mailbox,
            next_request_id: 1,
            stopping,
            #[cfg(test)]
            collector: Some(_collector),
        })
    }

    /// Serial caller API: a successful return means a completion watch is
    /// registered. A saturated worker still returns an ID; its ordered
    /// completion contains `Error::Busy`.
    pub(crate) fn submit(&mut self, operation: Operation) -> Result<u64, Error> {
        let request_id = self.next_request_id;
        self.next_request_id = request_id.checked_add(1).ok_or(Error::Stopped)?;
        let completed = self.completed.as_ref().ok_or(Error::Stopped)?.clone();
        let worker = self.worker.as_ref().ok_or(Error::Stopped)?;
        match worker.submit(operation, move |receiver| {
            match completed.try_send((request_id, receiver)) {
                Ok(()) => true,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
            }
        }) {
            Ok(()) | Err(Error::Busy | Error::Stopped) => Ok(request_id),
            Err(error) => Err(error),
        }
    }

    /// Drains all currently completed operations in worker order. The lock is
    /// held only while moving bounded in-memory values.
    pub(crate) fn poll(&self) -> Vec<Completion> {
        match self.mailbox.0.try_lock() {
            Ok(mut entries) => {
                let ready = entries.drain(..).collect();
                self.mailbox.1.notify_all();
                ready
            }
            Err(TryLockError::Poisoned(_)) | Err(TryLockError::WouldBlock) => Vec::new(),
        }
    }

    /// Test-only blocking shutdown. Completions are consumed during shutdown to prevent a full
    /// mailbox from stalling the worker.
    #[cfg(test)]
    pub(crate) fn finish(mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        self.completed.take();
        let worker = thread::spawn(move || worker.finish());
        if let Some(collector) = self.collector.take() {
            while !collector.is_finished() {
                if let Ok(mut entries) = self.mailbox.0.lock() {
                    entries.clear();
                    self.mailbox.1.notify_all();
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
            if collector.join().is_err() {
                tracing::warn!("Team completion collector panicked");
            }
        }
        self.stopping.store(true, Ordering::Relaxed);
        if worker.join().is_err() {
            tracing::warn!("Team worker joiner panicked");
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        self.mailbox.1.notify_all();
        // Worker and collector handles detach. Closing the channels lets the
        // worker drain accepted operations without joining on the UI thread.
    }
}

fn collect(incoming: Receiver<Watched>, mailbox: Mailbox, stopping: Arc<AtomicBool>) {
    while !stopping.load(Ordering::Relaxed) {
        let Ok((request_id, receiver)) = incoming.recv() else {
            break;
        };
        let result = receiver
            .recv()
            .unwrap_or_else(|_| Err(Error::Stopped))
            .map_err(Arc::new);
        let (entries, available) = (&mailbox.0, &mailbox.1);
        let mut entries = match entries.lock() {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while entries.len() >= COMPLETIONS && !stopping.load(Ordering::Relaxed) {
            entries = match available.wait(entries) {
                Ok(entries) => entries,
                Err(_) => return,
            };
        }
        if stopping.load(Ordering::Relaxed) {
            return;
        }
        entries.push_back(Completion { request_id, result });
    }
}

#[cfg(test)]
#[path = "team_service/tests.rs"]
mod tests;
