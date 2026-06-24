// Main application: eframe::App implementation.
// Manages all UI state, dispatches copy operations, handles benchmark results,
// and renders the two-panel layout.

use crate::benchmark::{BenchmarkResult, BenchmarkStatus, DiskBenchmark};
use crate::config::Config;
use crate::engine::copy_item::{long_path, CopyItem, CopyMode, CopyStatus};
use crate::engine::worker::{CopyControl, CopyOrchestrator, WorkerMessage};
use crate::engine::CopyJournal;
use crate::gui::source_tree::{SourceList, SourceNode};
use crate::gui::style;
use crossbeam_channel::Receiver;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Phases the application can be in.
#[derive(Debug, Clone, PartialEq)]
enum AppPhase {
    /// Idle: user is configuring sources and destination.
    Idle,
    /// Copying in progress.
    Copying,
    /// Paused mid-copy.
    Paused,
    /// Copy completed (success or with errors).
    Done,
}

/// Per-file display state in the copy queue panel.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct QueueEntry {
    source_name: String,
    size: u64,
    mode: CopyMode,
    status: CopyStatus,
    bytes_copied: u64,
}

/// The main application state.
pub struct FastCopyApp {
    // -- Configuration --
    config: Config,

    // -- Source/Destination --
    source_list: SourceList,
    dest_path: String,
    dest_free_space: Option<u64>,

    // -- Benchmark --
    benchmark_status: BenchmarkStatus,
    benchmark_thread: Option<std::thread::JoinHandle<Result<BenchmarkResult, String>>>,

    // -- Copy state --
    phase: AppPhase,
    queue: Vec<QueueEntry>,
    copy_control: Option<Arc<CopyControl>>,
    message_rx: Option<Receiver<WorkerMessage>>,
    total_bytes: u64,
    total_bytes_copied: u64,
    files_completed: usize,
    files_failed: usize,
    files_skipped: usize,
    copy_start_time: Option<Instant>,
    last_speed_bytes: u64,
    last_speed_time: Option<Instant>,
    current_speed: f64,
    errors: Vec<String>,

    // -- UI state --
    show_config: bool,
    config_threshold_mib: String,
    config_threads: String,
    config_unbuf_mib: String,
    config_buf_kib: String,
}

impl FastCopyApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = Config::load();
        let threshold_mib = (config.size_threshold_bytes / (1024 * 1024)).to_string();
        let threads = config.thread_count.to_string();
        let unbuf_mib = (config.unbuffered_buffer_bytes / (1024 * 1024)).to_string();
        let buf_kib = (config.buffered_buffer_bytes / 1024).to_string();

        Self {
            config,
            source_list: SourceList::new(),
            dest_path: String::new(),
            dest_free_space: None,
            benchmark_status: BenchmarkStatus::NotRun,
            benchmark_thread: None,
            phase: AppPhase::Idle,
            queue: Vec::new(),
            copy_control: None,
            message_rx: None,
            total_bytes: 0,
            total_bytes_copied: 0,
            files_completed: 0,
            files_failed: 0,
            files_skipped: 0,
            copy_start_time: None,
            last_speed_bytes: 0,
            last_speed_time: None,
            current_speed: 0.0,
            errors: Vec::new(),
            show_config: false,
            config_threshold_mib: threshold_mib,
            config_threads: threads,
            config_unbuf_mib: unbuf_mib,
            config_buf_kib: buf_kib,
        }
    }

    /// Process incoming messages from worker threads.
    fn process_messages(&mut self) {
        let rx = match &self.message_rx {
            Some(rx) => rx.clone(),
            None => return,
        };

        // Drain all pending messages.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                WorkerMessage::Progress {
                    index,
                    bytes_copied,
                    total_bytes: _,
                } => {
                    if let Some(entry) = self.queue.get_mut(index) {
                        let prev = entry.bytes_copied;
                        entry.bytes_copied = bytes_copied;
                        entry.status = CopyStatus::InProgress { bytes_copied };
                        // Update global counter.
                        if bytes_copied > prev {
                            self.total_bytes_copied += bytes_copied - prev;
                        }
                    }
                }
                WorkerMessage::FileCompleted { index } => {
                    if let Some(entry) = self.queue.get_mut(index) {
                        // Ensure we count remaining bytes.
                        let remaining = entry.size.saturating_sub(entry.bytes_copied);
                        self.total_bytes_copied += remaining;
                        entry.bytes_copied = entry.size;
                        entry.status = CopyStatus::Completed;
                    }
                    self.files_completed += 1;
                }
                WorkerMessage::FileFailed { index, error } => {
                    if let Some(entry) = self.queue.get_mut(index) {
                        entry.status = CopyStatus::Failed(error.clone());
                    }
                    self.files_failed += 1;
                    self.errors.push(error);
                }
                WorkerMessage::FileSkipped { index } => {
                    if let Some(entry) = self.queue.get_mut(index) {
                        entry.status = CopyStatus::Skipped;
                        self.total_bytes_copied += entry.size;
                    }
                    self.files_skipped += 1;
                }
                WorkerMessage::AllDone => {
                    self.phase = AppPhase::Done;
                }
            }
        }

        // Update speed calculation (every 500ms).
        let now = Instant::now();
        if let Some(last_time) = self.last_speed_time {
            let elapsed = now.duration_since(last_time).as_secs_f64();
            if elapsed >= 0.5 {
                let delta_bytes = self.total_bytes_copied.saturating_sub(self.last_speed_bytes);
                self.current_speed = delta_bytes as f64 / elapsed;
                self.last_speed_bytes = self.total_bytes_copied;
                self.last_speed_time = Some(now);
            }
        } else {
            self.last_speed_time = Some(now);
            self.last_speed_bytes = self.total_bytes_copied;
        }
    }

    /// Check if the benchmark thread has finished.
    fn check_benchmark(&mut self) {
        if self.benchmark_thread.is_none() {
            return;
        }

        let handle = self.benchmark_thread.as_ref().unwrap();
        if !handle.is_finished() {
            return;
        }

        let handle = self.benchmark_thread.take().unwrap();
        match handle.join() {
            Ok(Ok(result)) => {
                self.config.size_threshold_bytes = result.threshold_bytes;
                self.config.thread_count = result.recommended_threads;
                self.config_threshold_mib =
                    (result.threshold_bytes / (1024 * 1024)).to_string();
                self.config_threads = result.recommended_threads.to_string();
                let _ = self.config.save();
                self.benchmark_status = BenchmarkStatus::Completed(result);
            }
            Ok(Err(e)) => {
                self.benchmark_status = BenchmarkStatus::Failed(e);
            }
            Err(_) => {
                self.benchmark_status =
                    BenchmarkStatus::Failed("Benchmark thread panicked".to_string());
            }
        }
    }

    /// Start the disk benchmark in a background thread.
    fn start_benchmark(&mut self) {
        if self.dest_path.is_empty() {
            self.benchmark_status =
                BenchmarkStatus::Failed("Set a destination first".to_string());
            return;
        }

        let dest = PathBuf::from(&self.dest_path);
        if !dest.exists() {
            self.benchmark_status = BenchmarkStatus::Failed(format!(
                "Destination does not exist: {}",
                self.dest_path
            ));
            return;
        }

        self.benchmark_status = BenchmarkStatus::Running;

        let handle = std::thread::spawn(move || {
            let bench = DiskBenchmark::new(dest, None);
            bench.run()
        });

        self.benchmark_thread = Some(handle);
    }

    /// Start the copy operation.
    fn start_copy(&mut self) {
        if self.dest_path.is_empty() || self.source_list.is_empty() {
            return;
        }

        let dest_base = PathBuf::from(&self.dest_path);
        let threshold = self.config.size_threshold_bytes;

        // Build the copy queue from included source files.
        let files = self.source_list.collect_all_included();
        if files.is_empty() {
            return;
        }

        let mut items = Vec::new();
        let mut queue_entries = Vec::new();

        for (src_path, size) in &files {
            // Compute the destination path by preserving the relative structure.
            // For each root, the file is placed under dest/<root_name>/relative/path.
            let dest_file = compute_destination(src_path, &self.source_list, &dest_base);
            let src_long = long_path(src_path);
            let dst_long = long_path(&dest_file);

            let item = CopyItem::new(src_long, dst_long, *size, threshold);
            queue_entries.push(QueueEntry {
                source_name: src_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                size: *size,
                mode: item.mode,
                status: CopyStatus::Pending,
                bytes_copied: 0,
            });
            items.push(item);
        }

        self.queue = queue_entries;
        self.total_bytes = files.iter().map(|(_, s)| s).sum();
        self.total_bytes_copied = 0;
        self.files_completed = 0;
        self.files_failed = 0;
        self.files_skipped = 0;
        self.current_speed = 0.0;
        self.errors.clear();

        let (tx, rx) = crossbeam_channel::unbounded();
        let journal_path = CopyJournal::default_path();

        match CopyOrchestrator::new(self.config.clone(), journal_path, tx) {
            Ok(orchestrator) => {
                self.copy_control = Some(Arc::clone(&orchestrator.control));
                self.message_rx = Some(rx);
                self.copy_start_time = Some(Instant::now());
                self.last_speed_time = None;
                self.phase = AppPhase::Copying;
                orchestrator.start(items);
            }
            Err(e) => {
                self.errors.push(format!("Failed to start copy: {}", e));
            }
        }
    }

    /// Render the top bar: destination folder picker + free space indicator.
    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Destination:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.dest_path)
                    .desired_width(400.0)
                    .hint_text("Select destination folder..."),
            );
            if response.changed() {
                self.update_free_space();
            }

            if ui.button("Browse...").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.dest_path = path.to_string_lossy().to_string();
                    self.update_free_space();
                }
            }

            ui.separator();

            // Free space indicator.
            if let Some(free) = self.dest_free_space {
                ui.label(
                    egui::RichText::new(format!("Free: {}", style::format_bytes(free)))
                        .font(egui::FontId::new(13.0, egui::FontFamily::Monospace))
                        .color(style::TEXT_SECONDARY),
                );
            } else if !self.dest_path.is_empty() {
                ui.label(
                    egui::RichText::new("(space unknown)")
                        .color(style::TEXT_SECONDARY),
                );
            }
        });
    }

    fn update_free_space(&mut self) {
        let path = PathBuf::from(&self.dest_path);
        if path.exists() {
            // Use platform-specific free space query.
            self.dest_free_space = get_free_space_portable(&path);
        } else {
            self.dest_free_space = None;
        }
    }

    /// Render the left panel: source tree with checkboxes.
    fn render_source_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sources");
        ui.separator();

        // Add buttons.
        ui.horizontal(|ui| {
            if ui.button("+ Files").clicked() {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    for f in files {
                        self.source_list.add_file(f);
                    }
                }
            }
            if ui.button("+ Folder").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.source_list.add_directory(dir);
                }
            }
        });

        ui.add_space(4.0);

        // Summary line.
        let file_count = self.source_list.total_included_files();
        let total_size = self.source_list.total_included_size();
        ui.label(
            egui::RichText::new(format!(
                "{} files selected ({})",
                file_count,
                style::format_bytes(total_size)
            ))
            .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
            .color(style::TEXT_SECONDARY),
        );

        ui.separator();

        // Scrollable tree view.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut remove_index = None;
                for (idx, root) in self.source_list.roots.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("x")
                            .on_hover_text("Remove this source")
                            .clicked()
                        {
                            remove_index = Some(idx);
                        }
                        render_tree_node(ui, root);
                    });
                }
                if let Some(idx) = remove_index {
                    self.source_list.remove_root(idx);
                }
            });
    }

    /// Render the right panel: copy queue, progress, stats.
    fn render_queue_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Copy Queue");
        ui.separator();

        // Benchmark status bar.
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Benchmark: {}", self.benchmark_status))
                    .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                    .color(match &self.benchmark_status {
                        BenchmarkStatus::NotRun => style::WARNING,
                        BenchmarkStatus::Running => style::ACCENT,
                        BenchmarkStatus::Completed(_) => style::SUCCESS,
                        BenchmarkStatus::Failed(_) => style::ERROR,
                    }),
            );

            let can_bench = self.phase == AppPhase::Idle
                && !matches!(self.benchmark_status, BenchmarkStatus::Running);
            if ui
                .add_enabled(can_bench, egui::Button::new("Run Benchmark"))
                .clicked()
            {
                self.start_benchmark();
            }
        });

        ui.add_space(4.0);

        // Action buttons.
        ui.horizontal(|ui| {
            let can_copy = self.phase == AppPhase::Idle
                && !self.source_list.is_empty()
                && !self.dest_path.is_empty();
            if ui
                .add_enabled(
                    can_copy,
                    egui::Button::new(
                        egui::RichText::new("  Copy  ").color(egui::Color32::WHITE),
                    )
                    .fill(style::ACCENT),
                )
                .clicked()
            {
                self.start_copy();
            }

            match self.phase {
                AppPhase::Copying => {
                    if ui.button("Pause").clicked() {
                        if let Some(ctrl) = &self.copy_control {
                            ctrl.request_pause();
                            self.phase = AppPhase::Paused;
                        }
                    }
                }
                AppPhase::Paused => {
                    if ui.button("Resume").clicked() {
                        if let Some(ctrl) = &self.copy_control {
                            ctrl.resume();
                            self.phase = AppPhase::Copying;
                        }
                    }
                }
                _ => {}
            }

            if self.phase == AppPhase::Copying || self.phase == AppPhase::Paused {
                if ui
                    .button(egui::RichText::new("Cancel").color(style::ERROR))
                    .clicked()
                {
                    if let Some(ctrl) = &self.copy_control {
                        ctrl.request_cancel();
                    }
                    self.phase = AppPhase::Done;
                }
            }

            if self.phase == AppPhase::Done {
                if ui.button("Clear").clicked() {
                    self.queue.clear();
                    self.phase = AppPhase::Idle;
                    self.errors.clear();
                }
            }
        });

        ui.add_space(4.0);

        // Settings toggle.
        if ui
            .selectable_label(self.show_config, "Settings")
            .clicked()
        {
            self.show_config = !self.show_config;
        }

        if self.show_config {
            self.render_config_panel(ui);
        }

        ui.separator();

        // File queue list.
        if !self.queue.is_empty() {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(ui.available_height() - 100.0)
                .show(ui, |ui| {
                    for entry in &self.queue {
                        ui.horizontal(|ui| {
                            // Status icon.
                            let (icon, color) = match &entry.status {
                                CopyStatus::Pending => (".", style::TEXT_SECONDARY),
                                CopyStatus::InProgress { .. } => (">", style::ACCENT),
                                CopyStatus::Completed => ("v", style::SUCCESS),
                                CopyStatus::Failed(_) => ("!", style::ERROR),
                                CopyStatus::Skipped => ("-", style::TEXT_SECONDARY),
                                CopyStatus::Cancelled => ("x", style::WARNING),
                                CopyStatus::Paused { .. } => ("||", style::WARNING),
                            };
                            ui.label(
                                egui::RichText::new(icon)
                                    .color(color)
                                    .font(egui::FontId::new(
                                        13.0,
                                        egui::FontFamily::Monospace,
                                    )),
                            );

                            // File name and mode.
                            ui.label(&entry.source_name);
                            ui.label(
                                egui::RichText::new(format!(
                                    "[{}]",
                                    entry.mode
                                ))
                                .font(egui::FontId::new(
                                    11.0,
                                    egui::FontFamily::Monospace,
                                ))
                                .color(style::TEXT_SECONDARY),
                            );

                            // Size.
                            ui.label(
                                egui::RichText::new(style::format_bytes(entry.size))
                                    .font(egui::FontId::new(
                                        12.0,
                                        egui::FontFamily::Monospace,
                                    ))
                                    .color(style::TEXT_SECONDARY),
                            );

                            // Per-file progress bar for in-progress files.
                            if let CopyStatus::InProgress { .. } = &entry.status {
                                let frac = if entry.size > 0 {
                                    entry.bytes_copied as f32 / entry.size as f32
                                } else {
                                    1.0
                                };
                                ui.add(
                                    egui::ProgressBar::new(frac)
                                        .desired_width(80.0)
                                        .show_percentage(),
                                );
                            }
                        });
                    }
                });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("Add sources and click Copy to start")
                        .color(style::TEXT_SECONDARY),
                );
            });
        }
    }

    /// Render global progress stats at the bottom.
    fn render_bottom_bar(&self, ui: &mut egui::Ui) {
        if self.phase == AppPhase::Idle && self.queue.is_empty() {
            return;
        }

        ui.separator();

        // Global progress bar.
        let frac = if self.total_bytes > 0 {
            self.total_bytes_copied as f32 / self.total_bytes as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(frac)
                .show_percentage()
                .animate(self.phase == AppPhase::Copying),
        );

        ui.horizontal(|ui| {
            let mono = egui::FontId::new(13.0, egui::FontFamily::Monospace);

            // Speed.
            ui.label(
                egui::RichText::new(style::format_speed(self.current_speed))
                    .font(mono.clone())
                    .color(style::ACCENT),
            );

            ui.separator();

            // Copied / Total.
            ui.label(
                egui::RichText::new(format!(
                    "{} / {}",
                    style::format_bytes(self.total_bytes_copied),
                    style::format_bytes(self.total_bytes)
                ))
                .font(mono.clone())
                .color(style::TEXT_PRIMARY),
            );

            ui.separator();

            // ETA.
            let eta = if self.current_speed > 0.0 {
                let remaining = self.total_bytes.saturating_sub(self.total_bytes_copied);
                remaining as f64 / self.current_speed
            } else {
                -1.0
            };
            ui.label(
                egui::RichText::new(format!("ETA: {}", style::format_duration(eta)))
                    .font(mono.clone())
                    .color(style::TEXT_SECONDARY),
            );

            ui.separator();

            // File counts.
            let total_files = self.queue.len();
            ui.label(
                egui::RichText::new(format!(
                    "Done: {} | Failed: {} | Skipped: {} / {}",
                    self.files_completed, self.files_failed, self.files_skipped, total_files
                ))
                .font(mono)
                .color(style::TEXT_SECONDARY),
            );
        });

        // Show errors if any.
        if !self.errors.is_empty() {
            ui.add_space(4.0);
            ui.collapsing(
                egui::RichText::new(format!("{} error(s)", self.errors.len()))
                    .color(style::ERROR),
                |ui| {
                    for err in &self.errors {
                        ui.label(
                            egui::RichText::new(err)
                                .color(style::ERROR)
                                .font(egui::FontId::new(
                                    11.0,
                                    egui::FontFamily::Monospace,
                                )),
                        );
                    }
                },
            );
        }
    }

    /// Render the inline config/settings panel.
    fn render_config_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Configuration").strong());
            ui.add_space(2.0);

            egui::Grid::new("config_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Threshold (MiB):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config_threshold_mib)
                            .desired_width(60.0),
                    );
                    ui.end_row();

                    ui.label("Threads:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config_threads)
                            .desired_width(60.0),
                    );
                    ui.end_row();

                    ui.label("Unbuffered buffer (MiB):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config_unbuf_mib)
                            .desired_width(60.0),
                    );
                    ui.end_row();

                    ui.label("Buffered buffer (KiB):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config_buf_kib)
                            .desired_width(60.0),
                    );
                    ui.end_row();
                });

            ui.add_space(4.0);

            if ui.button("Apply & Save").clicked() {
                if let Ok(v) = self.config_threshold_mib.parse::<u64>() {
                    self.config.size_threshold_bytes = v * 1024 * 1024;
                }
                if let Ok(v) = self.config_threads.parse::<usize>() {
                    self.config.thread_count = v.max(1);
                }
                if let Ok(v) = self.config_unbuf_mib.parse::<usize>() {
                    self.config.unbuffered_buffer_bytes = v * 1024 * 1024;
                }
                if let Ok(v) = self.config_buf_kib.parse::<usize>() {
                    self.config.buffered_buffer_bytes = v * 1024;
                }
                let _ = self.config.save();
            }
        });
    }
}

impl eframe::App for FastCopyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply custom theme on every frame (cheap; just sets style).
        style::apply_theme(ctx);

        // Process worker messages and check benchmark.
        self.process_messages();
        self.check_benchmark();

        // Handle drag-and-drop of files/folders.
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                for file in &i.raw.dropped_files {
                    if let Some(path) = &file.path {
                        if path.is_dir() {
                            self.source_list.add_directory(path.clone());
                        } else {
                            self.source_list.add_file(path.clone());
                        }
                    }
                }
            }
        });

        // Request repaint while copying or benchmarking.
        if self.phase == AppPhase::Copying
            || self.phase == AppPhase::Paused
            || matches!(self.benchmark_status, BenchmarkStatus::Running)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // -- Layout --

        // Top bar.
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            self.render_top_bar(ui);
            ui.add_space(4.0);
        });

        // Bottom bar.
        egui::TopBottomPanel::bottom("bottom_bar")
            .min_height(0.0)
            .show(ctx, |ui| {
                self.render_bottom_bar(ui);
                ui.add_space(4.0);
            });

        // Left panel: sources.
        egui::SidePanel::left("source_panel")
            .default_width(300.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                self.render_source_panel(ui);
            });

        // Right/central panel: copy queue.
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_queue_panel(ui);
        });
    }
}

/// Recursively render a source tree node with a checkbox.
fn render_tree_node(ui: &mut egui::Ui, node: &mut SourceNode) {
    if node.is_dir {
        let id = ui.make_persistent_id(&node.path);
        let mut included = node.included;
        egui::CollapsingHeader::new("")
            .id_salt(id)
            .show_unindented(ui, |ui| {
                for child in &mut node.children {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        render_tree_node(ui, child);
                    });
                }
            });

        // Draw the checkbox+label inline with the header.
        ui.horizontal(|ui| {
            if ui.checkbox(&mut included, "").changed() {
                node.set_included_recursive(included);
            }
            ui.label(
                egui::RichText::new(&node.name)
                    .color(if included {
                        style::TEXT_PRIMARY
                    } else {
                        style::TEXT_SECONDARY
                    }),
            );
        });
    } else {
        ui.horizontal(|ui| {
            ui.checkbox(&mut node.included, "");
            ui.label(
                egui::RichText::new(&node.name).color(if node.included {
                    style::TEXT_PRIMARY
                } else {
                    style::TEXT_SECONDARY
                }),
            );
            ui.label(
                egui::RichText::new(style::format_bytes(node.size))
                    .font(egui::FontId::new(11.0, egui::FontFamily::Monospace))
                    .color(style::TEXT_SECONDARY),
            );
        });
    }
}

/// Compute the destination path for a source file, preserving relative structure.
/// For files added individually: placed directly under dest.
/// For files inside a directory root: placed under dest/<root_name>/relative/path.
fn compute_destination(
    src_path: &std::path::Path,
    source_list: &SourceList,
    dest_base: &std::path::Path,
) -> PathBuf {
    for root in &source_list.roots {
        if root.is_dir {
            if let Ok(relative) = src_path.strip_prefix(&root.path) {
                return dest_base.join(&root.name).join(relative);
            }
        }
    }
    // File added individually: just use the filename.
    let filename = src_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown"));
    dest_base.join(filename)
}

/// Cross-platform free space query.
fn get_free_space_portable(path: &std::path::Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let path_wide: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let mut free: u64 = 0;
        unsafe {
            if GetDiskFreeSpaceExW(
                PCWSTR(path_wide.as_ptr()),
                Some(&mut free),
                None,
                None,
            )
            .is_ok()
            {
                return Some(free);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        // On Linux, try reading from statvfs.
        let _ = path;
        None
    }
}
