// Win32 copy implementation using CopyFileExW.
//
// This module is only compiled on Windows. It uses the `windows` crate
// to call CopyFileExW with the appropriate flags:
// - Buffered mode: no special flags (standard OS-cached copy).
// - Unbuffered mode: COPY_FILE_NO_BUFFERING | COPY_FILE_RESTARTABLE.
//
// Pause and cancel are handled via the LPPROGRESS_ROUTINE callback:
// returning PROGRESS_CANCEL or PROGRESS_STOP as needed.
//
// Limitation: CopyFileExW preserves timestamps and standard attributes,
// but does NOT preserve ACLs. This is a documented known limitation.
// COPY_FILE_RESTARTABLE may reduce throughput due to restart-data bookkeeping.

#![cfg(windows)]

use crate::config::Config;
use crate::engine::copy_item::{CopyItem, CopyMode};
use crate::engine::worker::{CopyControl, WorkerMessage};
use crossbeam_channel::Sender;
use std::sync::Arc;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HANDLE, LARGE_INTEGER};
use windows::Win32::Storage::FileSystem::{
    CopyFileExW, COPY_FILE_NO_BUFFERING, COPY_FILE_RESTARTABLE,
    LPPROGRESS_ROUTINE_CALLBACK_REASON,
};

/// Progress callback return values (matching Win32 constants).
const PROGRESS_CONTINUE: u32 = 0;
const PROGRESS_CANCEL: u32 = 1;
const PROGRESS_STOP: u32 = 2;

/// Data passed to the progress callback via the lpData parameter.
struct ProgressContext {
    index: usize,
    total_bytes: u64,
    control: Arc<CopyControl>,
    tx: Sender<WorkerMessage>,
}

/// The progress routine called by CopyFileExW during the copy.
/// It reports progress and checks for pause/cancel requests.
unsafe extern "system" fn progress_routine(
    _total_file_size: LARGE_INTEGER,
    _total_bytes_transferred: LARGE_INTEGER,
    _stream_size: LARGE_INTEGER,
    stream_bytes_transferred: LARGE_INTEGER,
    _stream_number: u32,
    _callback_reason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _source_file: HANDLE,
    _destination_file: HANDLE,
    lp_data: *const std::ffi::c_void,
) -> u32 {
    if lp_data.is_null() {
        return PROGRESS_CONTINUE;
    }

    let ctx = &*(lp_data as *const ProgressContext);

    // Report progress.
    let transferred = stream_bytes_transferred as u64;
    let _ = ctx.tx.send(WorkerMessage::Progress {
        index: ctx.index,
        bytes_copied: transferred,
        total_bytes: ctx.total_bytes,
    });

    // Update global counter.
    ctx.control
        .total_bytes_copied
        .fetch_add(0, std::sync::atomic::Ordering::Relaxed);

    // Check cancel.
    if ctx.control.is_cancelled() {
        return PROGRESS_CANCEL;
    }

    // Check pause: PROGRESS_STOP tells CopyFileExW to pause (restartable).
    if ctx.control.is_paused() {
        return PROGRESS_STOP;
    }

    PROGRESS_CONTINUE
}

/// Copy a single file using Win32 CopyFileExW.
pub fn copy_file_win32(
    item: &CopyItem,
    _config: &Config,
    control: &Arc<CopyControl>,
    tx: &Sender<WorkerMessage>,
    index: usize,
) -> Result<(), String> {
    // Encode source and destination as null-terminated wide strings.
    let src_wide: Vec<u16> = item
        .source
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let dst_wide: Vec<u16> = item
        .destination
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // Choose copy flags based on the selected mode.
    let flags = match item.mode {
        CopyMode::Buffered => 0u32,
        CopyMode::Unbuffered => {
            (COPY_FILE_NO_BUFFERING.0 | COPY_FILE_RESTARTABLE.0) as u32
        }
    };

    let ctx = ProgressContext {
        index,
        total_bytes: item.size,
        control: Arc::clone(control),
        tx: tx.clone(),
    };

    let result = unsafe {
        CopyFileExW(
            PCWSTR(src_wide.as_ptr()),
            PCWSTR(dst_wide.as_ptr()),
            Some(progress_routine),
            Some(&ctx as *const ProgressContext as *const std::ffi::c_void),
            None, // pbCancel
            flags,
        )
    };

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let code = e.code();
            // ERROR_REQUEST_ABORTED (1235) is expected on cancel.
            if code.0 as u32 == 1235 {
                Err("Copy cancelled by user".to_string())
            } else {
                Err(format!("CopyFileExW failed: {} (HRESULT: 0x{:08X})", e.message(), code.0))
            }
        }
    }
}
