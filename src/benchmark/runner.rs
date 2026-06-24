// Benchmark runner: writes test files, measures buffered vs unbuffered throughput,
// and determines the crossover threshold.
//
// On non-Windows platforms, this module provides a stub that returns default values,
// since the actual benchmark relies on Win32 APIs (CopyFileExW with NO_BUFFERING).

use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Status of the benchmark process.
#[derive(Debug, Clone, PartialEq)]
pub enum BenchmarkStatus {
    NotRun,
    Running,
    Completed(BenchmarkResult),
    Failed(String),
}

impl std::fmt::Display for BenchmarkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchmarkStatus::NotRun => write!(f, "Not run"),
            BenchmarkStatus::Running => write!(f, "Running..."),
            BenchmarkStatus::Completed(r) => write!(
                f,
                "Completed (threshold = {} MiB, threads = {})",
                r.threshold_bytes / (1024 * 1024),
                r.recommended_threads
            ),
            BenchmarkStatus::Failed(e) => write!(f, "Failed: {}", e),
        }
    }
}

/// Cached benchmark result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkResult {
    /// Volume serial number that this result applies to.
    pub volume_serial: String,
    /// The determined crossover threshold in bytes.
    pub threshold_bytes: u64,
    /// Recommended number of threads.
    pub recommended_threads: usize,
    /// Timestamp when the benchmark was run.
    pub timestamp: String,
}

impl BenchmarkResult {
    /// Path to the cache file (next to the executable).
    pub fn cache_path() -> Option<PathBuf> {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join("fast-copy-benchmark.json")))
    }

    /// Load cached result for a given volume serial.
    #[allow(dead_code)]
    pub fn load_cached(volume_serial: &str) -> Option<Self> {
        let path = Self::cache_path()?;
        let data = std::fs::read_to_string(path).ok()?;
        let results: Vec<BenchmarkResult> = serde_json::from_str(&data).ok()?;
        results.into_iter().find(|r| r.volume_serial == volume_serial)
    }

    /// Save this result to the cache (appending/replacing by volume serial).
    pub fn save_to_cache(&self) -> Result<(), String> {
        let path = Self::cache_path()
            .ok_or_else(|| "Cannot determine cache path".to_string())?;

        let mut results: Vec<BenchmarkResult> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default();

        // Replace existing entry for the same volume, or append.
        if let Some(pos) = results.iter().position(|r| r.volume_serial == self.volume_serial) {
            results[pos] = self.clone();
        } else {
            results.push(self.clone());
        }

        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| format!("Serialize error: {}", e))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Write cache error: {}", e))
    }
}

/// The benchmark runner.
pub struct DiskBenchmark {
    /// Source directory to read test files from (optional; we write then read).
    #[allow(dead_code)]
    pub source_dir: Option<PathBuf>,
    /// Destination directory to write test files to.
    pub dest_dir: PathBuf,
}

impl DiskBenchmark {
    pub fn new(dest_dir: PathBuf, source_dir: Option<PathBuf>) -> Self {
        Self { source_dir, dest_dir }
    }

    /// Run the benchmark and return the result.
    /// On non-Windows, returns a default result since we cannot test CopyFileExW.
    pub fn run(&self) -> Result<BenchmarkResult, String> {
        // Ensure destination directory exists and is writable.
        if !self.dest_dir.exists() {
            return Err(format!("Destination {:?} does not exist", self.dest_dir));
        }

        // Check free space (need at least 200 MiB for test files).
        let free_space = get_free_space(&self.dest_dir);
        if free_space < 200 * 1024 * 1024 {
            return Err(format!(
                "Insufficient free space on destination volume ({} MiB free, need 200 MiB)",
                free_space / (1024 * 1024)
            ));
        }

        let volume_serial = get_volume_serial(&self.dest_dir);

        #[cfg(windows)]
        let result = self.run_windows_benchmark(&volume_serial)?;

        #[cfg(not(windows))]
        let result = self.run_stub_benchmark(&volume_serial)?;

        result.save_to_cache()?;
        Ok(result)
    }

    /// Stub benchmark for non-Windows: returns sensible defaults.
    #[cfg(not(windows))]
    fn run_stub_benchmark(&self, volume_serial: &str) -> Result<BenchmarkResult, String> {
        let config = Config::default();
        Ok(BenchmarkResult {
            volume_serial: volume_serial.to_string(),
            threshold_bytes: config.size_threshold_bytes,
            recommended_threads: config.thread_count,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Real Windows benchmark: writes test files of various sizes and measures
    /// buffered vs unbuffered copy throughput to find the crossover point.
    #[cfg(windows)]
    fn run_windows_benchmark(&self, volume_serial: &str) -> Result<BenchmarkResult, String> {
        use std::time::Instant;

        // Test file sizes: 256 KiB, 1 MiB, 4 MiB, 8 MiB, 16 MiB, 32 MiB, 64 MiB.
        let test_sizes: Vec<u64> = vec![
            256 * 1024,
            1024 * 1024,
            4 * 1024 * 1024,
            8 * 1024 * 1024,
            16 * 1024 * 1024,
            32 * 1024 * 1024,
            64 * 1024 * 1024,
        ];

        let test_dir = self.dest_dir.join(".fast-copy-benchmark");
        std::fs::create_dir_all(&test_dir)
            .map_err(|e| format!("Cannot create benchmark dir: {}", e))?;

        // Cleanup guard: ensure test files are removed even on error.
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(test_dir.clone());

        let mut crossover_threshold = 16 * 1024 * 1024u64; // Default if we cannot determine.

        for &size in &test_sizes {
            // Write a test file filled with pseudo-random data.
            let src_path = test_dir.join(format!("bench_src_{}.bin", size));
            let dst_buffered = test_dir.join(format!("bench_buf_{}.bin", size));
            let dst_unbuffered = test_dir.join(format!("bench_unbuf_{}.bin", size));

            write_test_file(&src_path, size)?;

            // Measure buffered copy.
            let buffered_time = {
                let start = Instant::now();
                copy_with_mode(&src_path, &dst_buffered, false)?;
                start.elapsed()
            };

            // Measure unbuffered copy.
            let unbuffered_time = {
                let start = Instant::now();
                copy_with_mode(&src_path, &dst_unbuffered, true)?;
                start.elapsed()
            };

            let _ = std::fs::remove_file(&dst_buffered);
            let _ = std::fs::remove_file(&dst_unbuffered);
            let _ = std::fs::remove_file(&src_path);

            // If unbuffered is faster at this size, it is the crossover point.
            if unbuffered_time < buffered_time {
                crossover_threshold = size;
                break;
            }
        }

        let threads = num_cpus::get().clamp(2, 8);

        Ok(BenchmarkResult {
            volume_serial: volume_serial.to_string(),
            threshold_bytes: crossover_threshold,
            recommended_threads: threads,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }
}

/// Write a test file of the given size filled with a repeating byte pattern.
#[cfg(windows)]
fn write_test_file(path: &Path, size: u64) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("Cannot create test file {:?}: {}", path, e))?;
    // Write in 1 MiB chunks.
    let chunk_size = 1024 * 1024usize;
    let chunk: Vec<u8> = (0..chunk_size).map(|i| (i % 256) as u8).collect();
    let mut remaining = size;
    while remaining > 0 {
        let to_write = (remaining as usize).min(chunk_size);
        file.write_all(&chunk[..to_write])
            .map_err(|e| format!("Write error: {}", e))?;
        remaining -= to_write as u64;
    }
    file.flush().map_err(|e| format!("Flush error: {}", e))?;
    Ok(())
}

/// Copy a file using CopyFileExW with or without NO_BUFFERING.
#[cfg(windows)]
fn copy_with_mode(src: &Path, dst: &Path, unbuffered: bool) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        CopyFileExW, COPY_FILE_NO_BUFFERING,
    };

    let src_wide: Vec<u16> = src
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let dst_wide: Vec<u16> = dst
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let flags = if unbuffered {
        COPY_FILE_NO_BUFFERING.0 as u32
    } else {
        0u32
    };

    unsafe {
        CopyFileExW(
            PCWSTR(src_wide.as_ptr()),
            PCWSTR(dst_wide.as_ptr()),
            None,
            None,
            None,
            flags,
        )
        .map_err(|e| format!("CopyFileExW benchmark failed: {}", e.message()))
    }
}

/// Get free disk space for the volume containing the given path.
/// Returns 0 on error or on non-Windows platforms.
fn get_free_space(path: &Path) -> u64 {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let path_wide: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes_available = 0u64;
        unsafe {
            let _ = GetDiskFreeSpaceExW(
                PCWSTR(path_wide.as_ptr()),
                Some(&mut free_bytes_available),
                None,
                None,
            );
        }
        free_bytes_available
    }
    #[cfg(not(windows))]
    {
        // On Linux, use statvfs.
        use std::ffi::CString;
        let c_path = match CString::new(path.to_string_lossy().as_bytes()) {
            Ok(p) => p,
            Err(_) => return 0,
        };
        unsafe {
            let mut stat: libc_statvfs = std::mem::zeroed();
            if statvfs(c_path.as_ptr(), &mut stat) == 0 {
                stat.f_bavail as u64 * stat.f_frsize as u64
            } else {
                // Fallback: assume enough space.
                u64::MAX
            }
        }
    }
}

// Minimal libc bindings for statvfs on non-Windows (avoids pulling in the libc crate).
#[cfg(not(windows))]
#[repr(C)]
#[allow(non_camel_case_types)]
struct libc_statvfs {
    f_bsize: u64,
    f_frsize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_favail: u64,
    f_fsid: u64,
    f_flag: u64,
    f_namemax: u64,
    __f_spare: [i32; 6],
}

#[cfg(not(windows))]
extern "C" {
    fn statvfs(path: *const std::ffi::c_char, buf: *mut libc_statvfs) -> i32;
}

/// Get a volume serial number string for caching purposes.
/// On non-Windows, returns a hash of the path.
fn get_volume_serial(path: &Path) -> String {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetVolumeInformationW;

        // Extract root path (e.g., "C:\\").
        let root = path
            .components()
            .next()
            .map(|c| {
                let mut s = c.as_os_str().to_string_lossy().to_string();
                if !s.ends_with('\\') {
                    s.push('\\');
                }
                s
            })
            .unwrap_or_else(|| "C:\\".to_string());

        let root_wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut serial: u32 = 0;

        unsafe {
            let _ = GetVolumeInformationW(
                PCWSTR(root_wide.as_ptr()),
                None,
                Some(&mut serial),
                None,
                None,
                None,
            );
        }

        format!("{:08X}", serial)
    }
    #[cfg(not(windows))]
    {
        // Simple hash of the mount point path.
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:016X}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_result_serialization() {
        let result = BenchmarkResult {
            volume_serial: "ABCD1234".to_string(),
            threshold_bytes: 16 * 1024 * 1024,
            recommended_threads: 4,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let loaded: BenchmarkResult = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.volume_serial, "ABCD1234");
        assert_eq!(loaded.threshold_bytes, 16 * 1024 * 1024);
    }

    #[test]
    fn test_benchmark_status_display() {
        assert_eq!(format!("{}", BenchmarkStatus::NotRun), "Not run");
        assert_eq!(format!("{}", BenchmarkStatus::Running), "Running...");
        assert!(format!("{}", BenchmarkStatus::Failed("oops".into())).contains("oops"));
    }

    #[test]
    fn test_volume_serial_non_windows() {
        // On non-Windows, should return some hex string.
        let serial = get_volume_serial(Path::new("/tmp"));
        assert!(!serial.is_empty());
    }
}
