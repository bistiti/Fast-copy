// Platform-independent copy orchestration: the file loop, cancellation checks,
// conflict policy, journal interaction, and partial-file cleanup. All platform
// I/O goes through the `FileCopier` trait, so this logic is unit-tested with a
// mock copier and temp directories (see the tests below).

use crate::engine::copier::{CopyOutcome, FileCopier};
use crate::engine::copy_item::CopyItem;
use crate::engine::journal::CopyJournal;
use crate::engine::worker::{ConflictPolicy, CopyControl};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Receives per-file progress and terminal events. The app sends these over a
/// channel; tests collect them in memory.
pub trait ItemReporter {
    fn progress(&self, index: usize, bytes_copied: u64);
    fn completed(&self, index: usize);
    fn failed(&self, index: usize, error: String);
    fn skipped(&self, index: usize);
}

/// Outcome of processing a single queue item.
#[derive(Debug, PartialEq, Eq)]
pub enum ItemResult {
    Copied,
    Skipped,
    Failed,
    Cancelled,
}

/// Aggregate result of a `run_copy` pass.
#[derive(Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // exercised by the unit tests
pub struct RunSummary {
    pub copied: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: bool,
}

/// Given a destination that already exists, find a unique sibling path by
/// appending " (1)", " (2)", ... before the extension. Preserves any \\?\ prefix.
pub fn unique_destination(path: &Path) -> PathBuf {
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

/// Best-effort removal of a partial destination left by a cancelled/failed copy.
/// The file is not journal-recorded, so removing it prevents a truncated file
/// from masquerading as complete and lets resume re-copy cleanly.
fn remove_partial(dest: &Path) {
    let _ = std::fs::remove_file(dest);
}

/// Process one item: honour cancel/pause, journal-skip, conflict policy, ensure
/// the destination directory, copy via `copier`, and journal success.
pub fn process_item(
    index: usize,
    item: &mut CopyItem,
    copier: &dyn FileCopier,
    control: &CopyControl,
    journal: &Mutex<CopyJournal>,
    conflict_policy: ConflictPolicy,
    reporter: &dyn ItemReporter,
) -> ItemResult {
    if control.is_cancelled() {
        return ItemResult::Cancelled;
    }
    wait_while_paused(control);
    if control.is_cancelled() {
        return ItemResult::Cancelled;
    }

    // Skip files already recorded in the journal (resume support).
    {
        let j = journal.lock().unwrap();
        if j.is_completed(&item.destination) {
            reporter.skipped(index);
            return ItemResult::Skipped;
        }
    }

    // Apply the conflict policy for destinations that already exist on disk.
    match conflict_policy {
        ConflictPolicy::Overwrite => {}
        ConflictPolicy::Skip => {
            if item.destination.exists() {
                reporter.skipped(index);
                return ItemResult::Skipped;
            }
        }
        ConflictPolicy::Rename => {
            if item.destination.exists() {
                item.destination = unique_destination(&item.destination);
            }
        }
    }

    if let Some(parent) = item.destination.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            reporter.failed(index, format!("Failed to create directory {:?}: {}", parent, e));
            return ItemResult::Failed;
        }
    }

    // Copy, retrying after a pause (COPY_FILE_RESTARTABLE resumes mid-file).
    loop {
        let mut cb = |b: u64| reporter.progress(index, b);
        match copier.copy_file(item, control, &mut cb) {
            CopyOutcome::Done => break,
            CopyOutcome::Paused => {
                wait_while_paused(control);
                if control.is_cancelled() {
                    remove_partial(&item.destination);
                    return ItemResult::Cancelled;
                }
                continue;
            }
            CopyOutcome::Cancelled => {
                remove_partial(&item.destination);
                return ItemResult::Cancelled;
            }
            CopyOutcome::Failed(e) => {
                remove_partial(&item.destination);
                reporter.failed(index, e);
                return ItemResult::Failed;
            }
        }
    }

    if let Ok(mut j) = journal.lock() {
        let _ = j.record_completed(&item.destination);
    }
    reporter.completed(index);
    ItemResult::Copied
}

fn wait_while_paused(control: &CopyControl) {
    while control.is_paused() && !control.is_cancelled() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Sequentially process every item, stopping as soon as a cancel is observed.
/// This is the deterministic core that the threaded orchestrator reuses per item.
#[allow(dead_code)] // the app uses the threaded path; this drives the unit tests
pub fn run_copy(
    items: &mut [CopyItem],
    copier: &dyn FileCopier,
    control: &CopyControl,
    journal: &Mutex<CopyJournal>,
    conflict_policy: ConflictPolicy,
    reporter: &dyn ItemReporter,
) -> RunSummary {
    let mut summary = RunSummary::default();
    for (index, item) in items.iter_mut().enumerate() {
        if control.is_cancelled() {
            summary.cancelled = true;
            break;
        }
        match process_item(index, item, copier, control, journal, conflict_policy, reporter) {
            ItemResult::Copied => summary.copied += 1,
            ItemResult::Skipped => summary.skipped += 1,
            ItemResult::Failed => summary.failed += 1,
            ItemResult::Cancelled => {
                summary.cancelled = true;
                break;
            }
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::sync::Arc;

    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A unique scratch directory for one test.
    fn temp_dir(tag: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "fast_copy_pipeline_{}_{}_{}",
            tag,
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, contents: &[u8]) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(contents).unwrap();
    }

    /// Build `n` source files and matching destination CopyItems under `dir`.
    fn make_items(dir: &Path, n: usize) -> (Vec<CopyItem>, Vec<PathBuf>) {
        let src_dir = dir.join("src");
        let dst_dir = dir.join("dst");
        std::fs::create_dir_all(&src_dir).unwrap();
        let mut items = Vec::new();
        let mut dsts = Vec::new();
        for i in 0..n {
            let src = src_dir.join(format!("file_{}.bin", i));
            let dst = dst_dir.join(format!("file_{}.bin", i));
            let contents = format!("contents of file {}", i).into_bytes();
            write_file(&src, &contents);
            items.push(CopyItem::new(
                src,
                dst.clone(),
                contents.len() as u64,
                1024 * 1024,
            ));
            dsts.push(dst);
        }
        (items, dsts)
    }

    fn journal_in(dir: &Path) -> Mutex<CopyJournal> {
        Mutex::new(CopyJournal::open(dir.join("journal.log")).unwrap())
    }

    #[derive(Default)]
    struct TestReporter {
        completed: Mutex<Vec<usize>>,
        failed: Mutex<Vec<usize>>,
        skipped: Mutex<Vec<usize>>,
    }
    impl ItemReporter for TestReporter {
        fn progress(&self, _index: usize, _bytes: u64) {}
        fn completed(&self, index: usize) {
            self.completed.lock().unwrap().push(index);
        }
        fn failed(&self, index: usize, _error: String) {
            self.failed.lock().unwrap().push(index);
        }
        fn skipped(&self, index: usize) {
            self.skipped.lock().unwrap().push(index);
        }
    }

    /// Configurable mock copier. Records what it copies and can trip cancel or
    /// simulate a partial mid-file abort.
    #[derive(Default)]
    struct MockCopier {
        copied: Mutex<Vec<PathBuf>>,
        attempts: AtomicUsize,
        /// After this many successful copies, trip the cancel flag.
        cancel_after: Option<usize>,
        /// At this 0-based attempt, write a partial file and return Cancelled.
        partial_cancel_at: Option<usize>,
        /// At this 0-based attempt, return Failed.
        fail_at: Option<usize>,
    }

    impl FileCopier for MockCopier {
        fn copy_file(
            &self,
            item: &CopyItem,
            control: &CopyControl,
            on_progress: &mut dyn FnMut(u64),
        ) -> CopyOutcome {
            let n = self.attempts.fetch_add(1, Ordering::SeqCst);

            if self.fail_at == Some(n) {
                return CopyOutcome::Failed("mock failure".to_string());
            }

            if self.partial_cancel_at == Some(n) {
                // Simulate an aborted large-file copy: leave a truncated file.
                write_file(&item.destination, b"PARTIAL-TRUNCATED-DATA");
                on_progress(8);
                control.request_cancel();
                return CopyOutcome::Cancelled;
            }

            // Normal full copy.
            std::fs::copy(&item.source, &item.destination).unwrap();
            on_progress(item.size);
            self.copied.lock().unwrap().push(item.destination.clone());

            if let Some(k) = self.cancel_after {
                if self.copied.lock().unwrap().len() >= k {
                    control.request_cancel();
                }
            }
            CopyOutcome::Done
        }
    }

    // ---- Test 1: cancellation between files ----
    #[test]
    fn cancel_between_files_stops_processing() {
        let dir = temp_dir("cancel_between");
        let (mut items, dsts) = make_items(&dir, 4);
        let copier = MockCopier {
            cancel_after: Some(1),
            ..Default::default()
        };
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = TestReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter,
        );

        // Exactly one file copied; the rest were never processed.
        assert_eq!(copier.copied.lock().unwrap().len(), 1);
        assert_eq!(summary.copied, 1);
        assert!(summary.cancelled);
        assert!(dsts[0].exists());
        assert!(!dsts[1].exists());
        assert!(!dsts[2].exists());
        assert!(!dsts[3].exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Test 2: cancellation produces no corrupt destination ----
    #[test]
    fn cancel_during_file_leaves_no_truncated_destination() {
        let dir = temp_dir("cancel_partial");
        let (mut items, dsts) = make_items(&dir, 3);
        let copier = MockCopier {
            partial_cancel_at: Some(0),
            ..Default::default()
        };
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = TestReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter,
        );

        assert!(summary.cancelled);
        // The partial destination must be cleaned up (not left truncated).
        assert!(!dsts[0].exists(), "partial destination should be removed");
        // And it must NOT be journal-recorded (so resume re-copies it).
        assert!(!journal.lock().unwrap().is_completed(&dsts[0]));
        // Remaining files untouched.
        assert!(!dsts[1].exists());
        assert!(!dsts[2].exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Test 3: resume correctness ----
    #[test]
    fn resume_skips_already_copied_and_writes_the_rest() {
        let dir = temp_dir("resume");
        let (mut items, dsts) = make_items(&dir, 4);
        let journal = journal_in(&dir);
        let control = CopyControl::new();

        // First pass: copy file 0, then cancel.
        let first = MockCopier {
            cancel_after: Some(1),
            ..Default::default()
        };
        let r1 = TestReporter::default();
        let s1 = run_copy(
            &mut items,
            &first,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &r1,
        );
        assert_eq!(s1.copied, 1);
        assert!(s1.cancelled);
        assert!(journal.lock().unwrap().is_completed(&dsts[0]));

        // Second pass (resume): fresh control + fresh copier.
        let control2 = CopyControl::new();
        let second = MockCopier::default();
        let r2 = TestReporter::default();
        let s2 = run_copy(
            &mut items,
            &second,
            &control2,
            &journal,
            ConflictPolicy::Overwrite,
            &r2,
        );

        // File 0 is skipped via the journal; files 1..3 are copied now.
        assert_eq!(s2.skipped, 1);
        assert_eq!(s2.copied, 3);
        assert!(!s2.cancelled);
        assert_eq!(*r2.skipped.lock().unwrap(), vec![0]);
        let copied2 = second.copied.lock().unwrap();
        assert_eq!(copied2.len(), 3);
        assert!(!copied2.contains(&dsts[0]));
        assert!(copied2.contains(&dsts[1]));
        assert!(copied2.contains(&dsts[2]));
        assert!(copied2.contains(&dsts[3]));
        assert!(dsts.iter().all(|d| d.exists()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Test 4: cancel-immediately then restart cleanly ----
    #[test]
    fn cancel_immediately_then_fresh_run_completes() {
        let dir = temp_dir("cancel_first");
        let (mut items, dsts) = make_items(&dir, 3);
        let journal = journal_in(&dir);

        // Cancel before the first file.
        let control = CopyControl::new();
        control.request_cancel();
        let copier = MockCopier::default();
        let r1 = TestReporter::default();
        let s1 = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &r1,
        );
        assert!(s1.cancelled);
        assert_eq!(s1.copied, 0);
        assert_eq!(copier.copied.lock().unwrap().len(), 0);
        // No partial artifacts and nothing journaled.
        assert!(dsts.iter().all(|d| !d.exists()));
        assert_eq!(journal.lock().unwrap().completed_count(), 0);

        // Fresh run completes everything correctly.
        let control2 = CopyControl::new();
        let copier2 = MockCopier::default();
        let r2 = TestReporter::default();
        let s2 = run_copy(
            &mut items,
            &copier2,
            &control2,
            &journal,
            ConflictPolicy::Overwrite,
            &r2,
        );
        assert_eq!(s2.copied, 3);
        assert!(!s2.cancelled);
        assert!(dsts.iter().all(|d| d.exists()));
        // Destination contents match their sources.
        for (item, dst) in items.iter().zip(dsts.iter()) {
            let src_bytes = std::fs::read(&item.source).unwrap();
            let dst_bytes = std::fs::read(dst).unwrap();
            assert_eq!(src_bytes, dst_bytes);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- A failed copy is also cleaned up and reported ----
    #[test]
    fn failed_copy_is_reported_and_cleaned_up() {
        let dir = temp_dir("fail");
        let (mut items, dsts) = make_items(&dir, 2);
        let copier = MockCopier {
            fail_at: Some(0),
            ..Default::default()
        };
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = TestReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter,
        );

        assert_eq!(summary.failed, 1);
        assert_eq!(summary.copied, 1);
        assert_eq!(*reporter.failed.lock().unwrap(), vec![0]);
        assert!(!dsts[0].exists());
        assert!(dsts[1].exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Conflict policy: rename keeps both ----
    #[test]
    fn rename_policy_keeps_both() {
        let dir = temp_dir("rename");
        let (mut items, dsts) = make_items(&dir, 1);
        // Pre-create the destination so the policy must rename.
        write_file(&dsts[0], b"existing");

        let copier = MockCopier::default();
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = TestReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Rename,
            &reporter,
        );

        assert_eq!(summary.copied, 1);
        // Original kept; a renamed sibling was written.
        assert!(dsts[0].exists());
        assert_ne!(items[0].destination, dsts[0]);
        assert!(items[0].destination.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ================== Step 5: hot-path / responsiveness tests ==================

    /// Reporter that counts callbacks (to assert progress cadence / no stalls).
    #[derive(Default)]
    struct CountingReporter {
        progress_count: AtomicUsize,
        completed_count: AtomicUsize,
    }
    impl ItemReporter for CountingReporter {
        fn progress(&self, _index: usize, _bytes: u64) {
            self.progress_count.fetch_add(1, Ordering::SeqCst);
        }
        fn completed(&self, _index: usize) {
            self.completed_count.fetch_add(1, Ordering::SeqCst);
        }
        fn failed(&self, _index: usize, _error: String) {}
        fn skipped(&self, _index: usize) {}
    }

    /// Fast copier that performs no real file I/O: it just reports progress and
    /// counts. Used for the many-small-files profile so the test is bounded by
    /// CPU, not disk. Can trip cancel after a given number of files.
    #[derive(Default)]
    struct CountingCopier {
        copied: AtomicUsize,
        progress_calls: AtomicUsize,
        cancel_after: Option<usize>,
    }
    impl FileCopier for CountingCopier {
        fn copy_file(
            &self,
            item: &CopyItem,
            control: &CopyControl,
            on_progress: &mut dyn FnMut(u64),
        ) -> CopyOutcome {
            on_progress(item.size);
            self.progress_calls.fetch_add(1, Ordering::SeqCst);
            let n = self.copied.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(k) = self.cancel_after {
                if n >= k {
                    control.request_cancel();
                }
            }
            CopyOutcome::Done
        }
    }

    /// Copier that sleeps per file (so pause/resume timing is observable) and
    /// counts completed files. Pause is enforced by `process_item`, not here.
    struct SleepCopier {
        copied: AtomicUsize,
        ms: u64,
    }
    impl FileCopier for SleepCopier {
        fn copy_file(
            &self,
            item: &CopyItem,
            _control: &CopyControl,
            on_progress: &mut dyn FnMut(u64),
        ) -> CopyOutcome {
            std::thread::sleep(std::time::Duration::from_millis(self.ms));
            on_progress(item.size);
            self.copied.fetch_add(1, Ordering::SeqCst);
            CopyOutcome::Done
        }
    }

    /// Copier simulating a large file delivered in chunks, honouring cancel
    /// between chunks exactly like the CopyFileExW progress routine returning
    /// PROGRESS_CANCEL. Writes a growing partial destination so cleanup can be
    /// verified. Optionally trips cancel itself after a given chunk.
    struct ChunkedCopier {
        chunks: u64,
        cancel_at_chunk: Option<u64>,
    }
    impl FileCopier for ChunkedCopier {
        fn copy_file(
            &self,
            item: &CopyItem,
            control: &CopyControl,
            on_progress: &mut dyn FnMut(u64),
        ) -> CopyOutcome {
            if let Some(p) = item.destination.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let chunk = (item.size / self.chunks).max(1);
            let mut transferred: u64 = 0;
            let mut c: u64 = 0;
            while transferred < item.size {
                if control.is_cancelled() {
                    return CopyOutcome::Cancelled;
                }
                transferred = (transferred + chunk).min(item.size);
                c += 1;
                // Mimic real bytes landing on disk before completion.
                std::fs::write(&item.destination, vec![0u8; transferred as usize]).unwrap();
                on_progress(transferred);
                if self.cancel_at_chunk == Some(c) {
                    control.request_cancel();
                }
            }
            // Finalize with the real contents.
            std::fs::copy(&item.source, &item.destination).unwrap();
            CopyOutcome::Done
        }
    }

    /// Build `n` CopyItems with non-existent sources (the mock copier never reads
    /// them) so we can exercise 50k items without writing 50k source files.
    fn make_phantom_items(dir: &Path, n: usize) -> Vec<CopyItem> {
        let dst_dir = dir.join("dst");
        let src_dir = dir.join("src");
        let mut items = Vec::with_capacity(n);
        for i in 0..n {
            let src = src_dir.join(format!("f{}.bin", i));
            let dst = dst_dir.join(format!("f{}.bin", i));
            items.push(CopyItem::new(src, dst, 4096, 1024 * 1024));
        }
        items
    }

    // ---- Many small files: Stop halts the count within one file ----
    #[test]
    fn many_small_files_stop_within_one_file() {
        let dir = temp_dir("many_small_stop");
        let mut items = make_phantom_items(&dir, 50_000);
        let copier = CountingCopier {
            cancel_after: Some(1000),
            ..Default::default()
        };
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = TestReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter,
        );

        assert!(summary.cancelled);
        // Cancel was tripped while copying file #1000; processing stops before the
        // next file — the files-copied count does not keep climbing.
        assert_eq!(summary.copied, 1000);
        assert!(
            copier.copied.load(Ordering::SeqCst) <= 1001,
            "no more than one file copied after Stop was observed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Many small files: progress callback fires for every file (no stall) ----
    #[test]
    fn many_small_files_progress_callback_fires_for_every_file() {
        let dir = temp_dir("many_small_cadence");
        let n = 50_000usize;
        let mut items = make_phantom_items(&dir, n);
        let copier = CountingCopier::default();
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = CountingReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter,
        );

        assert_eq!(summary.copied, n);
        // The progress callback fired once per file and never stalled.
        assert_eq!(copier.progress_calls.load(Ordering::SeqCst), n);
        assert_eq!(reporter.progress_count.load(Ordering::SeqCst), n);
        assert_eq!(reporter.completed_count.load(Ordering::SeqCst), n);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Pause halts progress; Resume continues to completion ----
    #[test]
    fn pause_halts_progress_then_resume_completes() {
        let dir = temp_dir("pause_resume");
        let n = 40usize;
        let dir_for_thread = dir.clone();
        let control = Arc::new(CopyControl::new());
        let copier = Arc::new(SleepCopier {
            copied: AtomicUsize::new(0),
            ms: 5,
        });

        let c2 = Arc::clone(&control);
        let cp2 = Arc::clone(&copier);
        let handle = std::thread::spawn(move || {
            let mut items = make_phantom_items(&dir_for_thread, n);
            let journal = journal_in(&dir_for_thread);
            let reporter = TestReporter::default();
            run_copy(
                &mut items,
                &*cp2,
                &*c2,
                &journal,
                ConflictPolicy::Overwrite,
                &reporter,
            );
            cp2.copied.load(Ordering::SeqCst)
        });

        // Let several files copy, then pause.
        std::thread::sleep(std::time::Duration::from_millis(40));
        control.request_pause();
        // Let any in-flight file finish and the worker park.
        std::thread::sleep(std::time::Duration::from_millis(40));
        let a = copier.copied.load(Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(80));
        let b = copier.copied.load(Ordering::SeqCst);

        assert_eq!(a, b, "no files copied while paused");
        assert!(a > 0 && a < n, "pause took effect mid-run (got {a})");

        control.resume();
        let final_copied = handle.join().unwrap();
        assert_eq!(final_copied, n, "resume completed the remaining files");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Large file: cancel mid-file via the chunk callback is prompt + clean ----
    #[test]
    fn large_file_cancel_midfile_via_callback_is_prompt_and_clean() {
        let dir = temp_dir("large_cancel");
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src = src_dir.join("big.bin");
        let size: usize = 1 << 20; // 1 MiB
        write_file(&src, &vec![7u8; size]);
        let dst = dir.join("dst").join("big.bin");
        let mut items = vec![CopyItem::new(
            src.clone(),
            dst.clone(),
            size as u64,
            1024 * 1024,
        )];

        let copier = ChunkedCopier {
            chunks: 64,
            cancel_at_chunk: Some(4),
        };
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = CountingReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter,
        );

        assert!(summary.cancelled);
        // Cancel honoured within one chunk of being set (prompt mid-file abort).
        assert!(
            reporter.progress_count.load(Ordering::SeqCst) <= 6,
            "cancel should be honoured within a chunk, not at end-of-file"
        );
        // The partial destination must be removed — nothing that looks complete
        // is left truncated — and it must not be journal-recorded.
        assert!(!dst.exists(), "partial large-file destination must be removed");
        assert!(!journal.lock().unwrap().is_completed(&dst));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Mixed tree (varied sizes, subfolders) copies fully with matching bytes ----
    #[test]
    fn mixed_tree_copies_all_and_contents_match() {
        let dir = temp_dir("mixed");
        let src_dir = dir.join("src");
        let dst_dir = dir.join("dst");

        // (relative path, size) — includes a zero-byte file and a larger one.
        let spec: [(&str, usize); 5] = [
            ("a/small.txt", 10),
            ("a/empty.bin", 0),
            ("b/medium.bin", 4096),
            ("b/c/large.bin", 256 * 1024),
            ("root.dat", 1),
        ];

        let mut items = Vec::new();
        let mut dsts = Vec::new();
        for (rel, sz) in spec.iter() {
            let src = src_dir.join(rel);
            let dst = dst_dir.join(rel);
            // Deterministic, size-dependent contents.
            let contents: Vec<u8> = (0..*sz).map(|i| (i % 251) as u8).collect();
            write_file(&src, &contents);
            items.push(CopyItem::new(src, dst.clone(), *sz as u64, 1024 * 1024));
            dsts.push(dst);
        }

        let copier = MockCopier::default(); // performs a real fs::copy
        let control = CopyControl::new();
        let journal = journal_in(&dir);
        let reporter = TestReporter::default();

        let summary = run_copy(
            &mut items,
            &copier,
            &control,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter,
        );

        assert_eq!(summary.copied, spec.len());
        assert!(!summary.cancelled);
        for (item, dst) in items.iter().zip(dsts.iter()) {
            assert!(dst.exists(), "destination {:?} should exist", dst);
            let src_bytes = std::fs::read(&item.source).unwrap();
            let dst_bytes = std::fs::read(dst).unwrap();
            assert_eq!(src_bytes, dst_bytes, "contents must match for {:?}", dst);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Cancel mid-file leaves no partial, then a fresh restart completes ----
    #[test]
    fn cancel_then_restart_no_partial_and_completes() {
        let dir = temp_dir("cancel_restart");
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let mut srcs = Vec::new();
        let mut dsts = Vec::new();
        for i in 0..3 {
            let src = src_dir.join(format!("f{}.bin", i));
            // ~64 KiB each so the chunked copier delivers several chunks.
            write_file(&src, &vec![(i as u8).wrapping_add(1); 64 * 1024]);
            srcs.push(src);
            dsts.push(dir.join("dst").join(format!("f{}.bin", i)));
        }
        let build_items = || -> Vec<CopyItem> {
            srcs.iter()
                .zip(dsts.iter())
                .map(|(s, d)| CopyItem::new(s.clone(), d.clone(), 64 * 1024, 1024 * 1024))
                .collect()
        };

        let journal = journal_in(&dir);

        // First pass: cancel mid-file 0.
        let mut items1 = build_items();
        let copier1 = ChunkedCopier {
            chunks: 8,
            cancel_at_chunk: Some(2),
        };
        let control1 = CopyControl::new();
        let reporter1 = TestReporter::default();
        let s1 = run_copy(
            &mut items1,
            &copier1,
            &control1,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter1,
        );
        assert!(s1.cancelled);
        // No partial artifacts and nothing journaled.
        assert!(dsts.iter().all(|d| !d.exists()), "no partial files remain");
        assert_eq!(journal.lock().unwrap().completed_count(), 0);

        // Restart fresh: a full copy completes everything with matching bytes.
        let mut items2 = build_items();
        let copier2 = MockCopier::default();
        let control2 = CopyControl::new();
        let reporter2 = TestReporter::default();
        let s2 = run_copy(
            &mut items2,
            &copier2,
            &control2,
            &journal,
            ConflictPolicy::Overwrite,
            &reporter2,
        );
        assert_eq!(s2.copied, 3);
        assert!(!s2.cancelled);
        for (s, d) in srcs.iter().zip(dsts.iter()) {
            assert!(d.exists());
            assert_eq!(std::fs::read(s).unwrap(), std::fs::read(d).unwrap());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
