// Defines a single file to be copied, along with its mode selection and status.

use std::path::PathBuf;

/// Which I/O strategy will be used for this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMode {
    /// Standard buffered I/O via CopyFileExW (no special flags).
    Buffered,
    /// Direct I/O via CopyFileExW with COPY_FILE_NO_BUFFERING + COPY_FILE_RESTARTABLE.
    Unbuffered,
}

impl std::fmt::Display for CopyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyMode::Buffered => write!(f, "Buffered"),
            CopyMode::Unbuffered => write!(f, "Unbuffered"),
        }
    }
}

/// Current status of a single copy operation.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum CopyStatus {
    Pending,
    InProgress {
        bytes_copied: u64,
    },
    Completed,
    Failed(String),
    Skipped,
    Cancelled,
    Paused {
        bytes_copied: u64,
    },
}

impl std::fmt::Display for CopyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CopyStatus::Pending => write!(f, "Pending"),
            CopyStatus::InProgress { bytes_copied } => {
                write!(f, "In progress ({} bytes)", bytes_copied)
            }
            CopyStatus::Completed => write!(f, "Completed"),
            CopyStatus::Failed(msg) => write!(f, "Failed: {}", msg),
            CopyStatus::Skipped => write!(f, "Skipped"),
            CopyStatus::Cancelled => write!(f, "Cancelled"),
            CopyStatus::Paused { bytes_copied } => {
                write!(f, "Paused ({} bytes)", bytes_copied)
            }
        }
    }
}

/// Represents one file in the copy queue.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CopyItem {
    /// Absolute source path (with \\?\ prefix on Windows for long path support).
    pub source: PathBuf,
    /// Absolute destination path (with \\?\ prefix on Windows).
    pub destination: PathBuf,
    /// Size in bytes of the source file.
    pub size: u64,
    /// The I/O mode selected for this file based on the threshold.
    pub mode: CopyMode,
    /// Current copy status.
    pub status: CopyStatus,
}

impl CopyItem {
    /// Create a new CopyItem, selecting the mode based on the given threshold.
    /// Files strictly smaller than the threshold use buffered I/O;
    /// files at or above the threshold use unbuffered I/O.
    pub fn new(source: PathBuf, destination: PathBuf, size: u64, threshold: u64) -> Self {
        let mode = if size < threshold {
            CopyMode::Buffered
        } else {
            CopyMode::Unbuffered
        };
        Self {
            source,
            destination,
            size,
            mode,
            status: CopyStatus::Pending,
        }
    }
}

/// Prefix a path with \\?\ for long path support on Windows.
/// On non-Windows platforms, this is a no-op.
pub fn long_path(p: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if s.starts_with("\\\\?\\") {
            p.to_path_buf()
        } else {
            PathBuf::from(format!("\\\\?\\{}", s))
        }
    }
    #[cfg(not(windows))]
    {
        p.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_selection_below_threshold() {
        let item = CopyItem::new(
            PathBuf::from("a.txt"),
            PathBuf::from("b.txt"),
            1000,
            2000,
        );
        assert_eq!(item.mode, CopyMode::Buffered);
    }

    #[test]
    fn test_mode_selection_at_threshold() {
        let item = CopyItem::new(
            PathBuf::from("a.txt"),
            PathBuf::from("b.txt"),
            2000,
            2000,
        );
        assert_eq!(item.mode, CopyMode::Unbuffered);
    }

    #[test]
    fn test_mode_selection_above_threshold() {
        let item = CopyItem::new(
            PathBuf::from("a.txt"),
            PathBuf::from("b.txt"),
            5000,
            2000,
        );
        assert_eq!(item.mode, CopyMode::Unbuffered);
    }

    #[test]
    fn test_mode_display() {
        assert_eq!(format!("{}", CopyMode::Buffered), "Buffered");
        assert_eq!(format!("{}", CopyMode::Unbuffered), "Unbuffered");
    }

    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", CopyStatus::Pending), "Pending");
        assert_eq!(format!("{}", CopyStatus::Completed), "Completed");
        assert!(format!("{}", CopyStatus::Failed("err".into())).contains("err"));
    }

    #[cfg(not(windows))]
    #[test]
    fn test_long_path_noop_on_non_windows() {
        let p = PathBuf::from("/some/path");
        assert_eq!(long_path(&p), p);
    }
}
