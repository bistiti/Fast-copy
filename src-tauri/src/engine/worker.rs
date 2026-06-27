// Copy worker: the threaded orchestrator. It pulls items from a shared queue and
// runs each through the platform-independent `pipeline::process_item`, reporting
// progress over a channel. All real copying happens behind the `FileCopier`
// trait (see copier.rs), which keeps the orchestration testable.

use crate::config::Config;
use crate::engine::copier::SystemCopier;
use crate::engine::copy_item::CopyItem;
use crate::engine::journal::CopyJournal;
use crate::engine::pipeline::{process_item, ItemReporter, ItemResult};
use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Messages sent from the worker threads back to the UI.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WorkerMessage {
    /// Progress update for a specific file (by its index in the queue).
    Progress {
        index: usize,
        bytes_copied: u64,
        total_bytes: u64,
    },
    /// A file finished copying.
    FileCompleted { index: usize },
    /// A file failed.
    FileFailed { index: usize, error: String },
    /// A file was skipped (already in journal, or conflict policy = skip).
    FileSkipped { index: usize },
    /// All work is done.
    AllDone,
}

/// How to handle a destination file that already exists on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    /// Overwrite the existing file (CopyFileExW default behavior).
    #[default]
    Overwrite,
    /// Leave the existing file in place; report the source as skipped.
    Skip,
    /// Copy to a uniquely-renamed destination ("name (1).ext", ...).
    Rename,
}

/// Shared state for controlling copy operations (pause / cancel).
#[derive(Debug)]
#[allow(dead_code)]
pub struct CopyControl {
    pub cancel_requested: AtomicBool,
    pub pause_requested: AtomicBool,
    pub total_bytes_copied: AtomicU64,
}

#[allow(dead_code)]
impl CopyControl {
    pub fn new() -> Self {
        Self {
            cancel_requested: AtomicBool::new(false),
            pause_requested: AtomicBool::new(false),
            total_bytes_copied: AtomicU64::new(0),
        }
    }

    pub fn request_cancel(&self) {
        self.cancel_requested.store(true, Ordering::SeqCst);
    }

    pub fn request_pause(&self) {
        self.pause_requested.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.pause_requested.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_requested.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.pause_requested.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.cancel_requested.store(false, Ordering::SeqCst);
        self.pause_requested.store(false, Ordering::SeqCst);
        self.total_bytes_copied.store(0, Ordering::SeqCst);
    }
}

impl Default for CopyControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Reporter that forwards pipeline events onto the worker message channel.
struct ChannelReporter {
    tx: Sender<WorkerMessage>,
}

impl ItemReporter for ChannelReporter {
    fn progress(&self, index: usize, bytes_copied: u64) {
        let _ = self.tx.send(WorkerMessage::Progress {
            index,
            bytes_copied,
            total_bytes: 0,
        });
    }
    fn completed(&self, index: usize) {
        let _ = self.tx.send(WorkerMessage::FileCompleted { index });
    }
    fn failed(&self, index: usize, error: String) {
        let _ = self.tx.send(WorkerMessage::FileFailed { index, error });
    }
    fn skipped(&self, index: usize) {
        let _ = self.tx.send(WorkerMessage::FileSkipped { index });
    }
}

/// The copy orchestrator: dispatches a work queue across a thread pool.
pub struct CopyOrchestrator {
    pub config: Config,
    pub control: Arc<CopyControl>,
    pub journal: Arc<Mutex<CopyJournal>>,
    pub message_tx: Sender<WorkerMessage>,
    pub conflict_policy: ConflictPolicy,
}

impl CopyOrchestrator {
    pub fn new(
        config: Config,
        journal_path: PathBuf,
        message_tx: Sender<WorkerMessage>,
        conflict_policy: ConflictPolicy,
    ) -> Result<Self, String> {
        let journal = CopyJournal::open(journal_path)?;
        Ok(Self {
            config,
            control: Arc::new(CopyControl::new()),
            journal: Arc::new(Mutex::new(journal)),
            message_tx,
            conflict_policy,
        })
    }

    /// Start copying the given items on a thread pool. Returns immediately;
    /// progress and completion are reported via the message channel.
    pub fn start(&self, items: Vec<CopyItem>) {
        let thread_count = self.config.thread_count.max(1);
        let control = Arc::clone(&self.control);
        let journal = Arc::clone(&self.journal);
        let tx = self.message_tx.clone();
        let conflict_policy = self.conflict_policy;

        // Shared work queue of (index, item) pairs.
        let work: Vec<(usize, CopyItem)> = items.into_iter().enumerate().collect();
        let work = Arc::new(Mutex::new(work.into_iter()));

        let mut handles = Vec::with_capacity(thread_count);
        for _ in 0..thread_count {
            let work = Arc::clone(&work);
            let control = Arc::clone(&control);
            let journal = Arc::clone(&journal);
            let tx = tx.clone();

            let handle = std::thread::spawn(move || {
                let copier = SystemCopier;
                let reporter = ChannelReporter { tx };
                loop {
                    if control.is_cancelled() {
                        break;
                    }
                    let task = {
                        let mut queue = work.lock().unwrap();
                        queue.next()
                    };
                    let (index, mut item) = match task {
                        Some(t) => t,
                        None => break,
                    };
                    let result = process_item(
                        index,
                        &mut item,
                        &copier,
                        &control,
                        &journal,
                        conflict_policy,
                        &reporter,
                    );
                    if result == ItemResult::Cancelled {
                        break;
                    }
                }
            });
            handles.push(handle);
        }

        // Monitor thread: wait for all workers, then signal completion.
        let tx_done = tx.clone();
        std::thread::spawn(move || {
            for h in handles {
                let _ = h.join();
            }
            let _ = tx_done.send(WorkerMessage::AllDone);
        });
    }
}
