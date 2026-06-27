// Bridge thread: drains the engine's crossbeam channel and re-emits each event
// to the webview as a Tauri event. Also samples copy throughput on a fixed tick
// so the frontend can render a live speed/ETA/chart without per-message spam.

use crate::engine::worker::WorkerMessage;
use crossbeam_channel::Receiver;
use serde_json::json;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// How often to emit a throughput sample (ms).
const SAMPLE_INTERVAL_MS: u64 = 300;

/// Spawn the forwarder. Consumes `rx`; runs until `AllDone` or the channel closes.
pub fn spawn(app: AppHandle, rx: Receiver<WorkerMessage>, sizes: Vec<u64>) {
    std::thread::spawn(move || {
        let n = sizes.len();
        let total_bytes: u64 = sizes.iter().sum();
        let mut per_file: Vec<u64> = vec![0; n];
        let mut total_copied: u64 = 0;
        let mut files_done: usize = 0;
        let mut files_failed: usize = 0;
        let mut files_skipped: usize = 0;
        let mut errors: Vec<String> = Vec::new();

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
                    if let Some(slot) = per_file.get_mut(index) {
                        if bytes_copied > *slot {
                            total_copied += bytes_copied - *slot;
                            *slot = bytes_copied;
                        }
                        let _ = app.emit(
                            "copy://progress",
                            json!({ "index": index, "bytesCopied": *slot }),
                        );
                    }
                }
                Ok(WorkerMessage::FileCompleted { index }) => {
                    if let (Some(slot), Some(size)) = (per_file.get_mut(index), sizes.get(index)) {
                        total_copied += size.saturating_sub(*slot);
                        *slot = *size;
                    }
                    files_done += 1;
                    let _ = app.emit("copy://file-done", json!({ "index": index }));
                }
                Ok(WorkerMessage::FileFailed { index, error }) => {
                    files_failed += 1;
                    errors.push(error.clone());
                    let _ = app.emit(
                        "copy://file-failed",
                        json!({ "index": index, "error": error }),
                    );
                }
                Ok(WorkerMessage::FileSkipped { index }) => {
                    if let (Some(slot), Some(size)) = (per_file.get_mut(index), sizes.get(index)) {
                        total_copied += size.saturating_sub(*slot);
                        *slot = *size;
                    }
                    files_skipped += 1;
                    let _ = app.emit("copy://file-skipped", json!({ "index": index }));
                }
                Ok(WorkerMessage::AllDone) => {
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
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Fall through to emit a throughput sample below.
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    // Channel closed without an explicit AllDone — finalize anyway.
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

            // Emit a throughput sample on every tick.
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
                let _ = app.emit(
                    "copy://throughput",
                    json!({
                        "speed": speed,
                        "totalCopied": total_copied,
                        "totalBytes": total_bytes,
                        "eta": eta,
                        "filesDone": files_done,
                        "filesFailed": files_failed,
                        "filesSkipped": files_skipped,
                    }),
                );
                last_sample_time = now;
                last_sample_copied = total_copied;
            }
        }
    });
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
