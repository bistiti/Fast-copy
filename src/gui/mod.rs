// GUI module: eframe/egui-based user interface for Fast-copy.
// Layout: top bar (destination + free space), left panel (source tree),
// right panel (copy queue + progress), bottom bar (global stats).

pub mod app;
pub mod source_tree;
pub mod style;

pub use app::FastCopyApp;
