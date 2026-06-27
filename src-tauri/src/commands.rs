// Tauri command handlers: the IPC surface the React frontend invokes.

use crate::bridge;
use crate::config::Config;
use crate::dto::{DestinationInfo, QueueEntryDto, TreeDto};
use crate::engine::copy_item::{long_path, CopyItem};
use crate::engine::worker::{ConflictPolicy, CopyOrchestrator};
use crate::engine::CopyJournal;
use crate::sources::compute_destination;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

// ---- Source tree ----

#[tauri::command]
pub fn add_sources(paths: Vec<String>, state: State<AppState>) -> TreeDto {
    let mut sources = state.sources.lock().unwrap();
    for p in paths {
        sources.add_file(PathBuf::from(p));
    }
    TreeDto::from_list(&sources)
}

#[tauri::command]
pub fn add_directory(path: String, state: State<AppState>) -> TreeDto {
    let mut sources = state.sources.lock().unwrap();
    sources.add_directory(PathBuf::from(path));
    TreeDto::from_list(&sources)
}

/// Add a mix of files and folders (used by drag-and-drop, where the frontend
/// doesn't know which paths are directories). Each path is classified here.
#[tauri::command]
pub fn add_paths(paths: Vec<String>, state: State<AppState>) -> TreeDto {
    let mut sources = state.sources.lock().unwrap();
    for p in paths {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            sources.add_directory(pb);
        } else {
            sources.add_file(pb);
        }
    }
    TreeDto::from_list(&sources)
}

#[tauri::command]
pub fn remove_root(index: usize, state: State<AppState>) -> TreeDto {
    let mut sources = state.sources.lock().unwrap();
    sources.remove_root(index);
    TreeDto::from_list(&sources)
}

#[tauri::command]
pub fn toggle_node(path: String, included: bool, state: State<AppState>) -> TreeDto {
    let mut sources = state.sources.lock().unwrap();
    sources.set_included_for_path(Path::new(&path), included);
    TreeDto::from_list(&sources)
}

#[tauri::command]
pub fn clear_sources(state: State<AppState>) -> TreeDto {
    let mut sources = state.sources.lock().unwrap();
    sources.clear();
    TreeDto::from_list(&sources)
}

// ---- Destination ----

#[tauri::command]
pub fn set_destination(path: String, state: State<AppState>) -> DestinationInfo {
    let pb = PathBuf::from(&path);
    let free_space = if pb.exists() {
        get_free_space(&pb)
    } else {
        None
    };
    *state.destination.lock().unwrap() = Some(pb);
    DestinationInfo { free_space }
}

// ---- Config ----

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(config: Config, state: State<AppState>) -> Result<Config, String> {
    config.save()?;
    let mut guard = state.config.lock().unwrap();
    *guard = config;
    Ok(guard.clone())
}

// ---- Benchmark ----

#[tauri::command]
pub fn run_benchmark(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let dest = state
        .destination
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Set a destination first".to_string())?;
    if !dest.exists() {
        return Err(format!("Destination does not exist: {}", dest.display()));
    }

    let _ = app.emit("benchmark://status", serde_json::json!({ "state": "running" }));

    std::thread::spawn(move || {
        let result = crate::benchmark::DiskBenchmark::new(dest, None).run();
        match result {
            Ok(r) => {
                // Persist the auto-tuned values into the live config.
                let state = app.state::<AppState>();
                {
                    let mut cfg = state.config.lock().unwrap();
                    cfg.size_threshold_bytes = r.threshold_bytes;
                    cfg.thread_count = r.recommended_threads;
                    let _ = cfg.save();
                }
                let _ = app.emit(
                    "benchmark://status",
                    serde_json::json!({
                        "state": "completed",
                        "thresholdMib": r.threshold_bytes / (1024 * 1024),
                        "threads": r.recommended_threads,
                    }),
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "benchmark://status",
                    serde_json::json!({ "state": "failed", "message": e }),
                );
            }
        }
    });

    Ok(())
}

// ---- Copy ----

#[tauri::command]
pub fn start_copy(
    conflict_policy: ConflictPolicy,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<QueueEntryDto>, String> {
    let dest_base = state
        .destination
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No destination selected".to_string())?;

    let config = state.config.lock().unwrap().clone();
    let threshold = config.size_threshold_bytes;

    // Build the queue while holding the sources lock (needed for relative paths).
    let (items, entries, sizes) = {
        let sources = state.sources.lock().unwrap();
        let files = sources.collect_all_included();
        if files.is_empty() {
            return Err("No files selected to copy".to_string());
        }

        let mut items = Vec::with_capacity(files.len());
        let mut entries = Vec::with_capacity(files.len());
        let mut sizes = Vec::with_capacity(files.len());

        for (idx, (src_path, size)) in files.iter().enumerate() {
            let dest_file = compute_destination(src_path, &sources, &dest_base);
            let item = CopyItem::new(long_path(src_path), long_path(&dest_file), *size, threshold);
            let name = src_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            entries.push(QueueEntryDto::new(idx, name, *size, item.mode));
            sizes.push(*size);
            items.push(item);
        }
        (items, entries, sizes)
    };

    let journal_path = CopyJournal::default_path();
    let (tx, rx) = crossbeam_channel::unbounded();
    let orchestrator = CopyOrchestrator::new(config, journal_path, tx, conflict_policy)
        .map_err(|e| format!("Failed to start copy: {}", e))?;

    // Only start forwarding once the orchestrator is ready.
    bridge::spawn(app.clone(), rx, sizes);

    *state.copy_control.lock().unwrap() = Some(Arc::clone(&orchestrator.control));
    orchestrator.start(items);

    Ok(entries)
}

#[tauri::command]
pub fn pause(state: State<AppState>) {
    if let Some(ctrl) = state.copy_control.lock().unwrap().as_ref() {
        ctrl.request_pause();
    }
}

#[tauri::command]
pub fn resume(state: State<AppState>) {
    if let Some(ctrl) = state.copy_control.lock().unwrap().as_ref() {
        ctrl.resume();
    }
}

#[tauri::command]
pub fn cancel(state: State<AppState>) {
    if let Some(ctrl) = state.copy_control.lock().unwrap().as_ref() {
        ctrl.request_cancel();
    }
}

// ---- Helpers ----

/// Query free disk space for the volume containing `path`.
fn get_free_space(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let wide: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut free: u64 = 0;
        unsafe {
            if GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut free), None, None).is_ok() {
                return Some(free);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}
