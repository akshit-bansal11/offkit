//! Background work. Tools never block the UI thread: they hand work to `run_pool` and post
//! results back with `post`, which runs a closure on the Slint event loop.
//!
//! The pattern every tool follows:
//! - Address rows by a stable id, never by index: rows can be removed while work runs.
//! - Give each run of a row a fresh `Arc<CancelToken>` and keep it on the row. A posted
//!   result applies only if the row still holds that same token (`Arc::ptr_eq`), so a stale
//!   worker from an earlier run can never overwrite a newer run's state.
//! - Batch inputs (a multi-file drop arrives as one event per file) and feed each batch to one
//!   `run_pool`, so N files never mean N processes at once.

use std::collections::VecDeque;
use std::fmt;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

/// Runs `f` on the UI thread with the component, if it still exists.
pub fn post<C: slint::ComponentHandle + 'static>(
    weak: &slint::Weak<C>,
    f: impl FnOnce(C) + Send + 'static,
) {
    // Fails only once the event loop has quit, when there is no UI left to update.
    let _ = weak.upgrade_in_event_loop(f);
}

/// Processes `items` on at most `concurrency` worker threads (never more threads than items),
/// in order. Returns at once with the workers' handles, so a caller can join them on shutdown.
pub fn run_pool<T: Send + 'static>(
    items: Vec<T>,
    concurrency: usize,
    work: impl Fn(T) + Send + Sync + 'static,
) -> Vec<thread::JoinHandle<()>> {
    let threads = concurrency.max(1).min(items.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(items)));
    let work = Arc::new(work);
    (0..threads)
        .map(|_| {
            let (queue, work) = (queue.clone(), work.clone());
            thread::spawn(move || {
                loop {
                    let next = queue
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .pop_front();
                    match next {
                        Some(item) => work(item),
                        None => break,
                    }
                }
            })
        })
        .collect()
}

/// How a job ended when it didn't succeed. `Failed` carries the message its row shows.
#[derive(Debug)]
pub enum JobError {
    Cancelled,
    Failed(String),
}

impl JobError {
    pub fn failed(e: impl fmt::Display) -> Self {
        Self::Failed(e.to_string())
    }
}

/// Cancellation for one job. A job running an external process registers the child so
/// `cancel` can kill it immediately instead of waiting for the job to poll the flag.
#[derive(Default)]
pub struct CancelToken {
    cancelled: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl CancelToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self.lock_child().as_mut() {
            // Already exited is fine; there is nothing else to do if the kill fails.
            let _ = child.kill();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Hands the running child to the token; kills it at once if cancel already happened.
    pub fn attach(&self, child: Child) {
        let mut slot = self.lock_child();
        *slot = Some(child);
        if self.is_cancelled()
            && let Some(child) = slot.as_mut()
        {
            let _ = child.kill();
        }
    }

    /// Takes the child back to wait on it once its output is drained.
    pub fn detach(&self) -> Option<Child> {
        self.lock_child().take()
    }

    fn lock_child(&self) -> std::sync::MutexGuard<'_, Option<Child>> {
        self.child.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
