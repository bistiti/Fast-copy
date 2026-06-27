// Human-readable formatters, ported from the old Rust `style.rs` so display
// logic lives with the UI. Binary (KiB/MiB/...) units, matching the engine.

const KIB = 1024;
const MIB = 1024 * 1024;
const GIB = 1024 * 1024 * 1024;
const TIB = 1024 * 1024 * 1024 * 1024;

/** Format a byte count, e.g. "1.23 GiB". */
export function formatBytes(bytes: number): string {
  if (bytes >= TIB) return `${(bytes / TIB).toFixed(2)} TiB`;
  if (bytes >= GIB) return `${(bytes / GIB).toFixed(2)} GiB`;
  if (bytes >= MIB) return `${(bytes / MIB).toFixed(2)} MiB`;
  if (bytes >= KIB) return `${(bytes / KIB).toFixed(1)} KiB`;
  return `${Math.round(bytes)} B`;
}

/** Format throughput in bytes/second, e.g. "82.0 MiB/s". */
export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec >= MIB) return `${(bytesPerSec / MIB).toFixed(1)} MiB/s`;
  if (bytesPerSec >= KIB) return `${(bytesPerSec / KIB).toFixed(1)} KiB/s`;
  return `${Math.round(bytesPerSec)} B/s`;
}

/** Format a duration in seconds, e.g. "1h 1m 1s". Negative → "--". */
export function formatDuration(seconds: number): string {
  if (seconds < 0 || !Number.isFinite(seconds)) return "--";
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}
