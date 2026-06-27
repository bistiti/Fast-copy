// Shared, thread-safe progress state for the directory-scan phase, plus the
// rough ETA estimator. The scan worker bumps these counters per entry; a sampler
// thread reads them and emits `scan://progress` events.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Live scan counters. All accurate (the running sums), except the estimate,
/// which is derived separately via [`estimate`].
#[derive(Debug, Default)]
pub struct ScanProgress {
    pub files_found: AtomicU64,
    pub folders_found: AtomicU64,
    pub bytes_found: AtomicU64,
    /// Number of immediate top-level subfolders of the scanned root (T).
    pub top_level_total: AtomicU64,
    /// Top-level subfolders fully scanned so far (C).
    pub top_level_done: AtomicU64,
    pub current_path: Mutex<String>,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&self, size: u64) {
        self.files_found.fetch_add(1, Ordering::Relaxed);
        self.bytes_found.fetch_add(size, Ordering::Relaxed);
    }

    pub fn add_folder(&self, path: &str) {
        self.folders_found.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut p) = self.current_path.lock() {
            *p = path.to_string();
        }
    }

    pub fn set_top_level_total(&self, t: u64) {
        self.top_level_total.store(t, Ordering::Relaxed);
    }

    pub fn complete_top_level(&self) {
        self.top_level_done.fetch_add(1, Ordering::Relaxed);
    }
}

/// A rough scan estimate. Deliberately low-confidence; always presented with
/// a `~`/"approx." marker by the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanEstimate {
    pub eta_secs: f64,
    pub total_files_est: u64,
    pub total_bytes_est: u64,
}

/// Compute the rough estimate, or `None` when no number may be shown.
///
/// Rules (see the design spec): return `None` if `t == 0` (unreadable / none),
/// if `c == 0` (no top-level subfolder finished yet — this also covers `t == 1`,
/// whose `c` only reaches 1 at completion), or if elapsed/throughput is not yet
/// meaningful.
pub fn estimate(
    files_found: u64,
    bytes_found: u64,
    c: u64,
    t: u64,
    elapsed_secs: f64,
) -> Option<ScanEstimate> {
    if t == 0 || c == 0 || elapsed_secs <= 0.0 || files_found == 0 {
        return None;
    }

    let ratio = t as f64 / c as f64;
    let total_files_est = (files_found as f64 * ratio).ceil() as u64;
    let total_bytes_est = (bytes_found as f64 * ratio).ceil() as u64;

    let throughput = files_found as f64 / elapsed_secs; // files/sec
    if throughput <= 0.0 {
        return None;
    }
    let remaining = total_files_est.saturating_sub(files_found) as f64;
    let eta_secs = remaining / throughput;

    Some(ScanEstimate {
        eta_secs,
        total_files_est,
        total_bytes_est,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_estimate_when_t_unreadable() {
        assert_eq!(estimate(100, 1000, 0, 0, 1.0), None);
    }

    #[test]
    fn no_estimate_when_c_zero() {
        // T known but no top-level subfolder finished yet.
        assert_eq!(estimate(100, 1000, 0, 8, 1.0), None);
    }

    #[test]
    fn no_estimate_for_single_top_level_until_done() {
        // T == 1, C == 0 -> no number (C only hits 1 at completion).
        assert_eq!(estimate(100, 1000, 0, 1, 1.0), None);
    }

    #[test]
    fn no_estimate_without_elapsed() {
        assert_eq!(estimate(100, 1000, 2, 8, 0.0), None);
    }

    #[test]
    fn extrapolates_normally() {
        // 1 of 4 top-level folders done, 100 files / 1000 bytes so far in 2s.
        let e = estimate(100, 1000, 1, 4, 2.0).unwrap();
        assert_eq!(e.total_files_est, 400);
        assert_eq!(e.total_bytes_est, 4000);
        // throughput 50 files/s, remaining 300 -> 6s.
        assert!((e.eta_secs - 6.0).abs() < 1e-9);
    }
}
