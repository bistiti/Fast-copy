// Debug-gated tracing for diagnosing UI-responsiveness issues (Steps 2/6).
//
// Disabled by default. Enable by setting the environment variable
// `FASTCOPY_TRACE=1` before launching the app. When enabled, timestamped
// (epoch-ms) lines carrying the thread id are appended to
// `%TEMP%\fastcopy-trace.log` (or `$TMPDIR/fastcopy-trace.log`). Writing to a
// file means the trace survives a frozen / "Not responding" window, which is
// exactly the case-A scenario we need to observe.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Whether tracing is enabled, evaluated once from the environment.
fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("FASTCOPY_TRACE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Append one timestamped, thread-tagged line to the trace file. No-op unless
/// `FASTCOPY_TRACE` is set. Never panics: all I/O errors are swallowed.
pub fn log(msg: &str) {
    if !enabled() {
        return;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tid = std::thread::current().id();
    let line = format!("{ts} {tid:?} {msg}\n");

    let dir = std::env::var("TEMP")
        .or_else(|_| std::env::var("TMP"))
        .or_else(|_| std::env::var("TMPDIR"))
        .unwrap_or_else(|_| ".".to_string());
    let path = std::path::Path::new(&dir).join("fastcopy-trace.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}
