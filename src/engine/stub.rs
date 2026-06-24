// Stub copy implementation for non-Windows platforms.
// This allows the project to compile on Linux for development and testing,
// but the actual copy logic is Windows-only (via CopyFileExW).
// This stub uses std::fs::copy as a basic fallback.

#![cfg(not(windows))]

use crate::engine::copy_item::CopyItem;
use crate::engine::worker::{CopyControl, WorkerMessage};
use crossbeam_channel::Sender;
use std::sync::Arc;

/// Fallback copy using std::fs::copy. No progress reporting, no pause/cancel.
/// Only meant for cross-compilation testing; real usage requires Windows.
pub fn copy_file_stub(
    item: &CopyItem,
    control: &Arc<CopyControl>,
    tx: &Sender<WorkerMessage>,
    index: usize,
) -> Result<(), String> {
    if control.is_cancelled() {
        return Err("Copy cancelled".to_string());
    }

    let _ = tx.send(WorkerMessage::Progress {
        index,
        bytes_copied: 0,
        total_bytes: item.size,
    });

    std::fs::copy(&item.source, &item.destination)
        .map_err(|e| format!("std::fs::copy failed: {}", e))?;

    let _ = tx.send(WorkerMessage::Progress {
        index,
        bytes_copied: item.size,
        total_bytes: item.size,
    });

    Ok(())
}
