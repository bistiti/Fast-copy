// Copy worker: manages the thread pool and dispatches copy tasks.
// On Windows, actual copying is performed via the win32 module.
// On non-Windows (for compilation only), the stub module provides no-op implementations.

use crate::config::Config;
use crate::engine::copy_item::CopyItem;
use crate::engine::journal::CopyJournal;
use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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

/// Given a destination that already exists, find a unique sibling path by
/// appending " (1)", " (2)", ... before the extension. Preserves any \\?\ prefix.
fn unique_destination(path: &Path) -> PathBuf {
    let parent = path.parent().map(Path::to_path_buf);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_string());

    for n in 1..u64::MAX {
        let name = match &ext {
            Some(ext) => format!("{} ({}).{}", stem, n, ext),
            None => format!("{} ({})", stem, n),
        };
        let candidate = match &parent {
            Some(p) => p.join(&name),
            None => PathBuf::from(&name),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

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
    FileCompleted {
        index: usize,
    },
    /// A file failed.
    FileFailed {
        index: usize,
        error: String,
    },
    /// A file was skipped (already in journal).
    FileSkipped {
        index: usize,
    },
    /// All work is done.
    AllDone,
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

/// The copy orchestrator: builds the work queue and dispatches to threads.
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

    /// Start copying the given items on a thread pool.
    /// This function spawns threads and returns immediately.
    /// Progress and completion are reported via the message channel.
    pub fn start(&self, items: Vec<CopyItem>) {
        let thread_count = self.config.thread_count.max(1);
        let control = Arc::clone(&self.control);
        let journal = Arc::clone(&self.journal);
        let tx = self.message_tx.clone();
        let config = self.config.clone();
        let conflict_policy = self.conflict_policy;

        // Build a work queue: (index, item) pairs.
        let work: Vec<(usize, CopyItem)> = items.into_iter().enumerate().collect();
        let work = Arc::new(Mutex::new(work.into_iter().peekable()));

        // Spawn worker threads.
        let mut handles = Vec::with_capacity(thread_count);

        for _tid in 0..thread_count {
            let work = Arc::clone(&work);
            let control = Arc::clone(&control);
            let journal = Arc::clone(&journal);
            let tx = tx.clone();
            let config = config.clone();

            let handle = std::thread::spawn(move || {
                loop {
                    if control.is_cancelled() {
                        break;
                    }

                    // Spin-wait while paused (with a short sleep to avoid busy-waiting).
                    while control.is_paused() && !control.is_cancelled() {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }

                    // Grab next work item.
                    let task = {
                        let mut queue = work.lock().unwrap();
                        queue.next()
                    };

                    let (index, mut item) = match task {
                        Some(t) => t,
                        None => break,
                    };

                    // Check journal for already-completed files.
                    {
                        let j = journal.lock().unwrap();
                        if j.is_completed(&item.destination) {
                            let _ = tx.send(WorkerMessage::FileSkipped { index });
                            continue;
                        }
                    }

                    // Apply the conflict policy for destinations that already exist.
                    match conflict_policy {
                        ConflictPolicy::Overwrite => {}
                        ConflictPolicy::Skip => {
                            if item.destination.exists() {
                                let _ = tx.send(WorkerMessage::FileSkipped { index });
                                continue;
                            }
                        }
                        ConflictPolicy::Rename => {
                            if item.destination.exists() {
                                item.destination = unique_destination(&item.destination);
                            }
                        }
                    }

                    // Ensure destination directory exists.
                    if let Some(parent) = item.destination.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            let _ = tx.send(WorkerMessage::FileFailed {
                                index,
                                error: format!(
                                    "Failed to create directory {:?}: {}",
                                    parent, e
                                ),
                            });
                            continue;
                        }
                    }

                    // Perform the actual copy. If the copy was interrupted by
                    // a pause (PROGRESS_STOP), wait for resume and retry.
                    // COPY_FILE_RESTARTABLE means the OS can pick up where it
                    // left off on the next CopyFileExW call.
                    let result = loop {
                        let r = perform_copy(&item, &config, &control, &tx, index);
                        match &r {
                            Err(e) if is_pause_sentinel(e) => {
                                // Wait for pause to be lifted, then retry.
                                let _ = tx.send(WorkerMessage::Progress {
                                    index,
                                    bytes_copied: 0, // placeholder; real progress comes from callback
                                    total_bytes: item.size,
                                });
                                while control.is_paused() && !control.is_cancelled() {
                                    std::thread::sleep(std::time::Duration::from_millis(50));
                                }
                                if control.is_cancelled() {
                                    break Err("Copy cancelled by user".to_string());
                                }
                                continue;
                            }
                            _ => break r,
                        }
                    };

                    match result {
                        Ok(()) => {
                            // Record in journal.
                            if let Ok(mut j) = journal.lock() {
                                let _ = j.record_completed(&item.destination);
                            }
                            let _ = tx.send(WorkerMessage::FileCompleted { index });
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerMessage::FileFailed {
                                index,
                                error: e,
                            });
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Spawn a monitor thread that waits for all workers, then sends AllDone.
        let tx_done = tx.clone();
        std::thread::spawn(move || {
            for h in handles {
                let _ = h.join();
            }
            let _ = tx_done.send(WorkerMessage::AllDone);
        });
    }
}

/// Perform the actual copy of a single file.
/// On Windows, delegates to the win32 module.
/// On non-Windows (cross-compilation), uses the stub.
fn perform_copy(
    item: &CopyItem,
    _config: &Config,
    control: &Arc<CopyControl>,
    tx: &Sender<WorkerMessage>,
    index: usize,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::engine::win32::copy_file_win32(item, _config, control, tx, index)
    }
    #[cfg(not(windows))]
    {
        crate::engine::stub::copy_file_stub(item, control, tx, index)
    }
}

/// Check whether an error string is the pause sentinel from win32::copy_file_win32.
fn is_pause_sentinel(err: &str) -> bool {
    #[cfg(windows)]
    {
        err == crate::engine::win32::PAUSE_SENTINEL
    }
    #[cfg(not(windows))]
    {
        let _ = err;
        false
    }
}
