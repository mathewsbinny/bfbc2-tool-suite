//! The editor's background jobs as one type (round PR1b-0, audit AU5 ARC-7): [`Task`] runs a body on its own named
//! thread under DF1's guard ([`crate::export_ui::guarded`]: a panic becomes an error line), sends its one answer on its
//! own channel, and its poller says when it came - an answer, still running, or a worker that ended without one
//! (an error, never a hang). Each job used to make its channel, its thread and its dead-worker check by hand.
//!
//! What a site does with the answer (its project generation check, its messages) stays at the site. A job that streams
//! lines while it runs (a tool's output read from its pipes: `prepare`) keeps its own threads.

use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError, channel};
use std::time::Duration;

use crate::error::EditorError;

/// A background job and its answer (see the module).
#[derive(Debug)]
pub struct Task<T> {
    rx: Receiver<Result<T, EditorError>>,
    what: String,
}

impl<T: Send + 'static> Task<T> {
    /// Runs `body` on a thread named for `what` ("The preview", "Reading the content profiles"): its answer, or its
    /// panic as an error naming `what`.
    pub fn spawn(what: &str, body: impl FnOnce() -> Result<T, EditorError> + Send + 'static) -> Self {
        let (tx, rx) = channel();
        let name = what.to_owned();
        let spawned = std::thread::Builder::new()
            .name(format!("bc2-editor: {what}"))
            .spawn(move || {
                let _ = tx.send(crate::export_ui::guarded(&name, body));
            });
        if let Err(e) = spawned {
            // no thread: the answer is that error (the sender went with the closure; a new channel holds it)
            let (tx, rx) = channel();
            let _ = tx.send(Err(EditorError::Worker(format!("{what}: no worker thread ({e})"))));
            return Self {
                rx,
                what: what.to_owned(),
            };
        }
        Self {
            rx,
            what: what.to_owned(),
        }
    }

    /// [`Task::spawn`] for a body that cannot fail.
    pub fn spawn_value(what: &str, body: impl FnOnce() -> T + Send + 'static) -> Self {
        Self::spawn(what, move || Ok(body()))
    }
}

impl<T> Task<T> {
    /// The answer when it came (`None`: still running); a worker that ended without one is an error.
    #[must_use]
    pub fn poll(&self) -> Option<Result<T, EditorError>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(self.stopped())),
        }
    }

    /// Waits for the answer (headless runs and tests).
    ///
    /// # Errors
    /// The body's error, its panic, or a worker that ended without an answer.
    pub fn wait(&self) -> Result<T, EditorError> {
        self.rx.recv().unwrap_or_else(|_| Err(self.stopped()))
    }

    /// Waits for the answer at most `limit` (`None`: still running).
    #[must_use]
    pub fn wait_for(&self, limit: Duration) -> Option<Result<T, EditorError>> {
        match self.rx.recv_timeout(limit) {
            Ok(r) => Some(r),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Err(self.stopped())),
        }
    }

    /// What the job does, as its errors name it.
    #[must_use]
    pub fn what(&self) -> &str {
        &self.what
    }

    fn stopped(&self) -> EditorError {
        EditorError::Worker(format!(
            "{}: the worker stopped unexpectedly (an internal error; see the console)",
            self.what
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_task_answers_once_and_a_panic_is_an_error() {
        let t = Task::spawn_value("Adding", || 2 + 3);
        assert_eq!(t.wait().unwrap(), 5);
        // the answer was taken: once the worker is gone (its sender dropped just after the send), it says so
        assert!(matches!(t.wait(), Err(EditorError::Worker(_))));
        let t: Task<u32> = Task::spawn("Failing", || Err(EditorError::Tool("no".into())));
        assert!(matches!(t.wait(), Err(EditorError::Tool(m)) if m == "no"));
        let t: Task<u32> = Task::spawn("Panicking", || panic!("boom"));
        let e = t.wait().unwrap_err().to_string();
        assert!(e.contains("Panicking") && e.contains("boom"), "{e}");
        let slow = Task::spawn_value("Waiting", || std::thread::sleep(Duration::from_millis(300)));
        assert!(slow.poll().is_none());
        assert!(slow.wait_for(Duration::from_secs(30)).unwrap().is_ok());
        assert_eq!(slow.what(), "Waiting");
    }
}
