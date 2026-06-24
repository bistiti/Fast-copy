// Fast-copy: an adaptive Windows file copy utility.
//
// Chooses dynamically between buffered and unbuffered I/O per file,
// based on a threshold determined by a real disk benchmark.
// Small files are copied in parallel using a thread pool with buffered I/O.
// Large files use CopyFileExW with COPY_FILE_NO_BUFFERING for maximum throughput.
//
// See README.md for usage, build instructions, and known limitations.

mod benchmark;
mod config;
mod engine;
mod gui;

use gui::FastCopyApp;

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Fast-copy")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "Fast-copy",
        options,
        Box::new(|cc| Ok(Box::new(FastCopyApp::new(cc)))),
    )
}
