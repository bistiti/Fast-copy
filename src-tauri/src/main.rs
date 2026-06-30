// Fast-copy: an adaptive Windows file copy utility (Tauri 2 + React UI).
//
// Chooses dynamically between buffered and unbuffered I/O per file, based on a
// threshold determined by a real disk benchmark. Small files are copied in
// parallel with buffered I/O; large files use CopyFileExW with
// COPY_FILE_NO_BUFFERING for maximum throughput.
//
// This file wires the Rust copy engine to the web UI: it registers the managed
// AppState and the #[tauri::command] handlers, and lets the frontend drive the
// engine over Tauri's IPC while the bridge thread streams progress back as events.
//
// See README.md for usage, build instructions, and known limitations.

// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod benchmark;
mod bridge;
mod commands;
mod config;
mod dto;
mod engine;
mod scan_progress;
mod sources;
mod state;
mod trace;

use state::AppState;

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::add_sources,
            commands::add_directory,
            commands::add_paths,
            commands::remove_root,
            commands::toggle_node,
            commands::clear_sources,
            commands::set_destination,
            commands::get_config,
            commands::set_config,
            commands::run_benchmark,
            commands::start_copy,
            commands::pause,
            commands::resume,
            commands::cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fast-copy");
}
