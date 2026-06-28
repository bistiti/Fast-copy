// Stub copy implementation for non-Windows platforms.
// Lets the project compile and the orchestration tests run off-Windows; the
// real Windows path uses CopyFileExW (see win32.rs). Uses std::fs::copy.

#![cfg(not(windows))]

use crate::engine::copier::CopyOutcome;
use crate::engine::copy_item::CopyItem;
use crate::engine::worker::CopyControl;

/// Fallback copy using std::fs::copy. No mid-file pause/cancel granularity.
pub fn copy_file_stub(
    item: &CopyItem,
    control: &CopyControl,
    on_progress: &mut dyn FnMut(u64),
) -> CopyOutcome {
    if control.is_cancelled() {
        return CopyOutcome::Cancelled;
    }
    on_progress(0);
    match std::fs::copy(&item.source, &item.destination) {
        Ok(_) => {
            on_progress(item.size);
            CopyOutcome::Done
        }
        Err(e) => CopyOutcome::Failed(format!("std::fs::copy failed: {}", e)),
    }
}
