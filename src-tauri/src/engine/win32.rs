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

use crate::engine::copier::CopyOutcome;
use crate::engine::copy_item::{CopyItem, CopyMode};
use crate::engine::worker::CopyControl;

use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{CopyFileExW, LPPROGRESS_ROUTINE_CALLBACK_REASON};

/// Progress callback return values (matching Win32 constants).
const PROGRESS_CONTINUE: u32 = 0;
const PROGRESS_CANCEL: u32 = 1;
const PROGRESS_STOP: u32 = 2;

// CopyFileExW flag values from WinBase.h. The `windows` crate exposes these
// only under Win32::System::WindowsProgramming (an extra feature); they are
// defined locally here because the ABI values are stable.
const COPY_FILE_RESTARTABLE: u32 = 0x0000_0002;
const COPY_FILE_NO_BUFFERING: u32 = 0x0000_1000;

/// Data passed to the progress callback via the lpData parameter. The borrows
/// are valid because the callback only runs synchronously inside the CopyFileExW
/// call on the same thread, and both referents outlive that call.
struct ProgressContext<'a> {
    control: &'a CopyControl,
    on_progress: &'a mut dyn FnMut(u64),
}

/// The progress routine called by CopyFileExW during the copy. Reports progress
/// using total_bytes_transferred (monotonic across all NTFS streams) and checks
/// for pause/cancel requests.
unsafe extern "system" fn progress_routine(
    _total_file_size: i64,
    total_bytes_transferred: i64,
    _stream_size: i64,
    _stream_bytes_transferred: i64,
    _stream_number: u32,
    _callback_reason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _source_file: HANDLE,
    _destination_file: HANDLE,
    lp_data: *const std::ffi::c_void,
) -> u32 {
    if lp_data.is_null() {
        return PROGRESS_CONTINUE;
    }

    let ctx = &mut *(lp_data as *mut ProgressContext);

    // Report cumulative bytes (across all NTFS alternate data streams).
    (ctx.on_progress)(total_bytes_transferred as u64);

    if ctx.control.is_cancelled() {
        return PROGRESS_CANCEL;
    }
    // PROGRESS_STOP -> CopyFileExW returns ERROR_REQUEST_ABORTED; the caller
    // distinguishes pause from cancel by checking the pause flag.
    if ctx.control.is_paused() {
        return PROGRESS_STOP;
    }

    PROGRESS_CONTINUE
}

/// Copy a single file using Win32 CopyFileExW.
pub fn copy_file_win32(
    item: &CopyItem,
    control: &CopyControl,
    on_progress: &mut dyn FnMut(u64),
) -> CopyOutcome {
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

    let flags = match item.mode {
        CopyMode::Buffered => 0u32,
        CopyMode::Unbuffered => COPY_FILE_NO_BUFFERING | COPY_FILE_RESTARTABLE,
    };

    let mut ctx = ProgressContext {
        control,
        on_progress,
    };

    let result = unsafe {
        CopyFileExW(
            PCWSTR(src_wide.as_ptr()),
            PCWSTR(dst_wide.as_ptr()),
            Some(progress_routine),
            Some(&mut ctx as *mut ProgressContext as *const std::ffi::c_void),
            None, // pbCancel
            flags,
        )
    };

    match result {
        Ok(()) => CopyOutcome::Done,
        Err(e) => {
            let code = e.code();
            // ERROR_REQUEST_ABORTED (0x800704D3 / Win32 1235) is returned on
            // both PROGRESS_CANCEL and PROGRESS_STOP.
            if code.0 as u32 == 0x800704D3 || code.0 as i32 == 1235 {
                if control.is_paused() && !control.is_cancelled() {
                    CopyOutcome::Paused
                } else {
                    CopyOutcome::Cancelled
                }
            } else {
                CopyOutcome::Failed(format!(
                    "CopyFileExW failed: {} (HRESULT: 0x{:08X})",
                    e.message(),
                    code.0
                ))
            }
        }
    }
}
