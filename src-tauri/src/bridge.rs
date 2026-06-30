// Bridge thread: drains the engine's crossbeam channel and forwards progress to
// the webview. To keep the webview's single JS thread responsive under very
// large queues, per-file events are NOT emitted individually. Instead the bridge
// accumulates row deltas (latest-state-wins, coalesced per file) plus the live
// throughput aggregate and emits ONE `copy://batch` event per fixed tick. This
// bounds IPC traffic to ~3 events/sec regardless of file throughput, so a copy
// of 100k+ files cannot flood the webview event loop (the root cause of the
// "Not responding" freeze and the stalled count/ETA — see commit message).

use crate::engine::worker::WorkerMessage;
use crossbeam_channel::Receiver;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// How often to flush a batch (row deltas + throughput sample), in ms.
const SAMPLE_INTERVAL_MS: u64 = 300;

/// One pending per-file state change, coalesced within a tick (latest wins).
struct RowDelta {
    status: &'static str,
    bytes_copied: u64,
    error: Option<String>,
}

/// Spawn the forwarder. Consumes `rx`; runs until `AllDone` or the channel closes.
/// `folder_of[i]` maps file i to its folder; `folder_counts[f]` is the number of
/// files in folder f — used to report folders-done/total.
pub fn spawn(
    app: AppHandle,
    rx: Receiver<WorkerMessage>,
    sizes: Vec<u64>,
    folder_of: Vec<usize>,
    folder_counts: Vec<u32>,
) {
    std::thread::spawn(move || {
        crate::trace::log(&format!("bridge: start, files={}", sizes.len()));
        let n = sizes.len();
        let total_bytes: u64 = sizes.iter().sum();
        let mut per_file: Vec<u64> = vec![0; n];
        let mut total_copied: u64 = 0;
        let mut files_done: usize = 0;
        let mut files_failed: usize = 0;
        let mut files_skipped: usize = 0;
        let mut errors: Vec<String> = Vec::new();

        let folders_total = folder_counts.len();
        let mut folder_remaining = folder_counts;
        let mut folders_done: usize = 0;
        // Index of the most recently active file, for the "currently copying" line.
        let mut current_index: Option<usize> = None;

        // Per-tick coalesced row deltas, keyed by file index (latest state wins).
        let mut pending: HashMap<usize, RowDelta> = HashMap::new();

        let start = Instant::now();
        let mut last_sample_time = start;
        let mut last_sample_copied: u64 = 0;

        loop {
            match rx.recv_timeout(Duration::from_millis(SAMPLE_INTERVAL_MS)) {
                Ok(WorkerMessage::Progress {
                    index,
                    bytes_copied,
                    ..
                }) => {
                    current_index = Some(index);
                    if let Some(slot) = per_file.get_mut(index) {
                        if bytes_copied > *slot {
                            total_copied += bytes_copied - *slot;
                            *slot = bytes_copied;
                        }
                        // Coalesce: many CopyFileExW callbacks for one large file
                        // collapse to a single delta carrying the latest byte count.
                        pending.insert(
                            index,
                            RowDelta {
                                status: "inProgress",
                                bytes_copied: *slot,
                                error: None,
                            },
                        );
                    }
                }
                Ok(WorkerMessage::FileCompleted { index }) => {
                    if let (Some(slot), Some(size)) = (per_file.get_mut(index), sizes.get(index)) {
                        total_copied += size.saturating_sub(*slot);
                        *slot = *size;
                    }
                    files_done += 1;
                    finish_in_folder(index, &folder_of, &mut folder_remaining, &mut folders_done);
                    pending.insert(
                        index,
                        RowDelta {
                            status: "done",
                            bytes_copied: sizes.get(index).copied().unwrap_or(0),
                            error: None,
                        },
                    );
                }
                Ok(WorkerMessage::FileFailed { index, error }) => {
                    files_failed += 1;
                    finish_in_folder(index, &folder_of, &mut folder_remaining, &mut folders_done);
                    errors.push(error.clone());
                    pending.insert(
                        index,
                        RowDelta {
                            status: "failed",
                            bytes_copied: per_file.get(index).copied().unwrap_or(0),
                            error: Some(error),
                        },
                    );
                }
                Ok(WorkerMessage::FileSkipped { index }) => {
                    if let (Some(slot), Some(size)) = (per_file.get_mut(index), sizes.get(index)) {
                        total_copied += size.saturating_sub(*slot);
                        *slot = *size;
                    }
                    files_skipped += 1;
                    finish_in_folder(index, &folder_of, &mut folder_remaining, &mut folders_done);
                    pending.insert(
                        index,
                        RowDelta {
                            status: "skipped",
                            bytes_copied: sizes.get(index).copied().unwrap_or(0),
                            error: None,
                        },
                    );
                }
                Ok(WorkerMessage::AllDone) => {
                    crate::trace::log(&format!(
                        "bridge: AllDone received, files_done={files_done} failed={files_failed} skipped={files_skipped}"
                    ));
                    // Flush any rows accumulated since the last tick, plus a final
                    // throughput snapshot, before the terminal summary.
                    emit_batch(
                        &app,
                        &mut pending,
                        throughput_fields(
                            0.0,
                            total_copied,
                            total_bytes,
                            -1.0,
                            start.elapsed().as_secs_f64(),
                            files_done,
                            files_failed,
                            files_skipped,
                            folders_done,
                            folders_total,
                            current_index,
                        ),
                    );
                    emit_done(
                        &app,
                        total_copied,
                        total_bytes,
                        start.elapsed().as_secs_f64(),
                        files_done,
                        files_failed,
                        files_skipped,
                        &errors,
                    );
                    crate::trace::log("bridge: copy://done emitted");
                    break;
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Fall through to flush a batch below.
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    // Channel closed without an explicit AllDone — finalize anyway.
                    emit_batch(
                        &app,
                        &mut pending,
                        throughput_fields(
                            0.0,
                            total_copied,
                            total_bytes,
                            -1.0,
                            start.elapsed().as_secs_f64(),
                            files_done,
                            files_failed,
                            files_skipped,
                            folders_done,
                            folders_total,
                            current_index,
                        ),
                    );
                    emit_done(
                        &app,
                        total_copied,
                        total_bytes,
                        start.elapsed().as_secs_f64(),
                        files_done,
                        files_failed,
                        files_skipped,
                        &errors,
                    );
                    break;
                }
            }

            // Flush a single batched event on each tick: coalesced row deltas plus
            // the throughput aggregate. One emit per tick, independent of file count.
            let now = Instant::now();
            let dt = now.duration_since(last_sample_time).as_secs_f64();
            if dt >= (SAMPLE_INTERVAL_MS as f64) / 1000.0 {
                let speed = if dt > 0.0 {
                    (total_copied.saturating_sub(last_sample_copied)) as f64 / dt
                } else {
                    0.0
                };
                let eta = if speed > 0.0 {
                    total_bytes.saturating_sub(total_copied) as f64 / speed
                } else {
                    -1.0
                };
                emit_batch(
                    &app,
                    &mut pending,
                    throughput_fields(
                        speed,
                        total_copied,
                        total_bytes,
                        eta,
                        start.elapsed().as_secs_f64(),
                        files_done,
                        files_failed,
                        files_skipped,
                        folders_done,
                        folders_total,
                        current_index,
                    ),
                );
                last_sample_time = now;
                last_sample_copied = total_copied;
            }
        }
    });
}

/// Build the throughput aggregate object shared by every batch.
#[allow(clippy::too_many_arguments)]
fn throughput_fields(
    speed: f64,
    total_copied: u64,
    total_bytes: u64,
    eta: f64,
    elapsed_secs: f64,
    files_done: usize,
    files_failed: usize,
    files_skipped: usize,
    folders_done: usize,
    folders_total: usize,
    current_index: Option<usize>,
) -> Value {
    json!({
        "speed": speed,
        "totalCopied": total_copied,
        "totalBytes": total_bytes,
        "eta": eta,
        "elapsedSecs": elapsed_secs,
        "filesDone": files_done,
        "filesFailed": files_failed,
        "filesSkipped": files_skipped,
        "foldersDone": folders_done,
        "foldersTotal": folders_total,
        "currentIndex": current_index,
    })
}

/// Emit one `copy://batch`: drains the pending row deltas into an array and
/// pairs them with the throughput aggregate. Always emitted on a tick (even with
/// zero rows) so count/ETA keep refreshing.
fn emit_batch(app: &AppHandle, pending: &mut HashMap<usize, RowDelta>, throughput: Value) {
    let rows: Vec<Value> = pending
        .drain()
        .map(|(index, d)| {
            json!({
                "index": index,
                "status": d.status,
                "bytesCopied": d.bytes_copied,
                "error": d.error,
            })
        })
        .collect();
    let _ = app.emit("copy://batch", json!({ "rows": rows, "throughput": throughput }));
}

/// Decrement a folder's remaining-file count when one of its files reaches a
/// terminal state; count the folder done when it reaches zero.
fn finish_in_folder(
    index: usize,
    folder_of: &[usize],
    folder_remaining: &mut [u32],
    folders_done: &mut usize,
) {
    if let Some(&f) = folder_of.get(index) {
        if let Some(rem) = folder_remaining.get_mut(f) {
            if *rem > 0 {
                *rem -= 1;
                if *rem == 0 {
                    *folders_done += 1;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_done(
    app: &AppHandle,
    total_copied: u64,
    total_bytes: u64,
    elapsed_secs: f64,
    files_done: usize,
    files_failed: usize,
    files_skipped: usize,
    errors: &[String],
) {
    let avg_speed = if elapsed_secs > 0.0 {
        total_copied as f64 / elapsed_secs
    } else {
        0.0
    };
    let _ = app.emit(
        "copy://done",
        json!({
            "totalCopied": total_copied,
            "totalBytes": total_bytes,
            "elapsedSecs": elapsed_secs,
            "avgSpeed": avg_speed,
            "filesDone": files_done,
            "filesFailed": files_failed,
            "filesSkipped": files_skipped,
            "errors": errors,
        }),
    );
}
