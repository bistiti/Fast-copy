// Custom dark theme styling for the Fast-copy UI.
// Colors, spacings, rounding, and font configuration.

use egui::{Color32, FontFamily, FontId, Rounding, Stroke, Style, TextStyle, Visuals};

/// Accent color used for highlights, buttons, and progress bars.
pub const ACCENT: Color32 = Color32::from_rgb(86, 156, 214);

/// Slightly dimmed accent for hover states.
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(106, 176, 234);

/// Background for the main panels.
pub const BG_DARK: Color32 = Color32::from_rgb(30, 30, 30);

/// Slightly lighter background for panels and cards.
pub const BG_PANEL: Color32 = Color32::from_rgb(37, 37, 38);

/// Background for input fields and list items.
pub const BG_INPUT: Color32 = Color32::from_rgb(45, 45, 48);

/// Primary text color.
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(212, 212, 212);

/// Secondary/dimmed text color.
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 150, 150);

/// Success green.
pub const SUCCESS: Color32 = Color32::from_rgb(78, 201, 176);

/// Error red.
pub const ERROR: Color32 = Color32::from_rgb(244, 71, 71);

/// Warning yellow.
pub const WARNING: Color32 = Color32::from_rgb(220, 180, 50);

/// Apply the custom dark theme to an egui context.
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = Style::default();

    // Use dark visuals as a base.
    style.visuals = Visuals::dark();

    // Customize colors.
    style.visuals.panel_fill = BG_DARK;
    style.visuals.window_fill = BG_PANEL;
    style.visuals.extreme_bg_color = BG_INPUT;

    // Widget visuals.
    style.visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    style.visuals.widgets.noninteractive.rounding = Rounding::same(4.0);

    style.visuals.widgets.inactive.bg_fill = BG_INPUT;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    style.visuals.widgets.inactive.rounding = Rounding::same(4.0);

    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(55, 55, 60);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, ACCENT_HOVER);
    style.visuals.widgets.hovered.rounding = Rounding::same(4.0);

    style.visuals.widgets.active.bg_fill = ACCENT;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.active.rounding = Rounding::same(4.0);

    style.visuals.selection.bg_fill = ACCENT.linear_multiply(0.3);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Generous spacing.
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);

    // Text styles: use monospace for sizes/speeds, proportional for everything else.
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(18.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.0, FontFamily::Monospace),
    );

    ctx.set_style(style);
}

/// Format a byte count as a human-readable string (e.g., "1.23 GiB").
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * 1024 * 1024;
    const TIB: u64 = 1024 * 1024 * 1024 * 1024;

    if bytes >= TIB {
        format!("{:.2} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format a throughput in bytes/second as "XX.X MiB/s".
pub fn format_speed(bytes_per_sec: f64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes_per_sec >= MIB {
        format!("{:.1} MiB/s", bytes_per_sec / MIB)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.1} KiB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

/// Format a duration as "Xh Xm Xs" or "Xm Xs" or "Xs".
pub fn format_duration(seconds: f64) -> String {
    if seconds < 0.0 {
        return "--".to_string();
    }
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(1048576), "1.00 MiB");
        assert_eq!(format_bytes(1073741824), "1.00 GiB");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(500.0), "500 B/s");
        assert_eq!(format_speed(1024.0 * 1024.0), "1.0 MiB/s");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0.0), "0s");
        assert_eq!(format_duration(65.0), "1m 5s");
        assert_eq!(format_duration(3661.0), "1h 1m 1s");
        assert_eq!(format_duration(-1.0), "--");
    }
}
