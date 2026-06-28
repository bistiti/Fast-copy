// The platform boundary for the file-copy primitive.
//
// All OS-specific copying (Windows CopyFileExW, std::fs elsewhere) sits behind
// the `FileCopier` trait so the orchestration in `pipeline` can be unit-tested
// against an in-memory/mock implementation with no real platform I/O.

use crate::engine::copy_item::CopyItem;
use crate::engine::worker::CopyControl;

/// Result of attempting to copy one file.
#[derive(Debug)]
pub enum CopyOutcome {
    /// The file was copied successfully.
    Done,
    /// The copy was interrupted by a pause request; the caller should retry
    /// after the pause is lifted.
    Paused,
    /// The copy was interrupted by a cancel request.
    Cancelled,
    /// The copy failed with the given message.
    Failed(String),
}

/// The single file-copy primitive. `on_progress` is invoked with the cumulative
/// number of bytes copied for this file. Implementations must honour `control`
/// (cancel / pause) where the platform allows it.
pub trait FileCopier: Send + Sync {
    fn copy_file(
        &self,
        item: &CopyItem,
        control: &CopyControl,
        on_progress: &mut dyn FnMut(u64),
    ) -> CopyOutcome;
}

/// The real copier: CopyFileExW on Windows, std::fs::copy elsewhere.
pub struct SystemCopier;

impl FileCopier for SystemCopier {
    fn copy_file(
        &self,
        item: &CopyItem,
        control: &CopyControl,
        on_progress: &mut dyn FnMut(u64),
    ) -> CopyOutcome {
        #[cfg(windows)]
        {
            crate::engine::win32::copy_file_win32(item, control, on_progress)
        }
        #[cfg(not(windows))]
        {
            crate::engine::stub::copy_file_stub(item, control, on_progress)
        }
    }
}
