//! How a long use case runs (ADR-0014): it can be cancelled, it reports progress, and it may
//! use several threads.
//!
//! Every use case that can take long receives a [`RunContext`]:
//!
//! - **Cancellation is cooperative.** Use cases and adapters check the [`CancellationToken`]
//!   between units of work (a file, a directory entry) and inside long ones (a parser checks
//!   while it parses). A cancelled run returns [`Cancelled`] and no partial result: a half-built
//!   inventory or report must never pass for a complete one.
//! - **Progress** is reported per unit of work as a [`Progress`]. The runtime turns it into the
//!   contract's `begin` / `report` / `end` notifications; this layer does not know the wire
//!   format.
//! - **Threads.** Ports are `Send + Sync`, so a use case may call them from several threads.
//!   [`RunContext::workers`] says how many; it comes from the composition root (the CLI or
//!   runtime asks the operating system), because this layer reads nothing from its environment.
//!   Results never depend on the worker count.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};

/// Driven port: tells a running use case to stop.
pub trait CancellationToken: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

/// Driven port: receives progress. It is called from worker threads, once per unit of work,
/// so it must be cheap; an implementation that sends notifications should throttle them.
pub trait ProgressReporter: Send + Sync {
    fn report(&self, progress: Progress);
}

/// A stage of an analysis, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// Walking the repository and hashing source files.
    Scan,
    /// Parsing files and extracting their symbols.
    Parse,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Scan => "scan",
            Phase::Parse => "parse",
        }
    }
}

/// `done` units of `phase` are finished, out of `total` when the total is known in advance
/// (parsing knows it from the inventory; scanning does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub phase: Phase,
    pub done: usize,
    pub total: Option<usize>,
}

/// The run was cancelled before it finished. Nothing it produced is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the run was cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// A [`CancellationToken`] that is never cancelled.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeverCancelled;

impl CancellationToken for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// A [`ProgressReporter`] that discards progress.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoProgress;

impl ProgressReporter for NoProgress {
    fn report(&self, _progress: Progress) {}
}

/// A [`CancellationToken`] that another thread can set, for example the runtime when the client
/// sends `$/cancelRequest`. Once set it stays set.
#[derive(Debug, Default)]
pub struct CancelFlag(AtomicBool);

impl CancelFlag {
    pub fn new() -> Self {
        CancelFlag::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl CancellationToken for CancelFlag {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Cancellation, progress and thread count for one run of a use case.
#[derive(Clone, Copy)]
pub struct RunContext<'a> {
    cancel: &'a dyn CancellationToken,
    progress: &'a dyn ProgressReporter,
    workers: NonZeroUsize,
}

impl<'a> RunContext<'a> {
    /// A run on one thread.
    pub fn new(cancel: &'a dyn CancellationToken, progress: &'a dyn ProgressReporter) -> Self {
        RunContext {
            cancel,
            progress,
            workers: NonZeroUsize::MIN,
        }
    }

    /// A run that cannot be cancelled and reports nothing, on one thread: for tests and tools.
    pub fn detached() -> RunContext<'static> {
        RunContext::new(&NeverCancelled, &NoProgress)
    }

    /// Use up to `workers` threads.
    pub fn with_workers(self, workers: NonZeroUsize) -> Self {
        RunContext { workers, ..self }
    }

    pub fn workers(&self) -> NonZeroUsize {
        self.workers
    }

    pub fn cancellation(&self) -> &'a dyn CancellationToken {
        self.cancel
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// `Err(Cancelled)` once the run is cancelled; call it between units of work.
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn report(&self, phase: Phase, done: usize, total: Option<usize>) {
        self.progress.report(Progress { phase, done, total });
    }
}

impl std::fmt::Debug for RunContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunContext")
            .field("cancelled", &self.is_cancelled())
            .field("workers", &self.workers)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_flag_stays_cancelled() {
        let flag = CancelFlag::new();
        let run = RunContext::new(&flag, &NoProgress);
        assert_eq!(run.check(), Ok(()));
        flag.cancel();
        assert_eq!(run.check(), Err(Cancelled));
        assert!(run.is_cancelled());
    }

    #[test]
    fn a_detached_run_uses_one_thread_and_never_stops() {
        let run = RunContext::detached();
        assert_eq!(run.workers().get(), 1);
        assert_eq!(run.check(), Ok(()));
        let wider = run.with_workers(NonZeroUsize::new(8).unwrap());
        assert_eq!(wider.workers().get(), 8);
    }

    #[test]
    fn phases_use_stable_names() {
        assert_eq!(
            [Phase::Scan.as_str(), Phase::Parse.as_str()],
            ["scan", "parse"]
        );
        assert!(Phase::Scan < Phase::Parse);
    }
}
