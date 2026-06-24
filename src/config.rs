// Configuration management: loading, saving, and defaults for all tunable parameters.
// The config file is stored next to the executable as "fast-copy.json".

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// All user-tunable settings. Serialized to JSON next to the executable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Size threshold in bytes: files below this use buffered I/O,
    /// files at or above this use unbuffered I/O.
    pub size_threshold_bytes: u64,

    /// Buffer size for unbuffered (direct) I/O copies.
    /// Must be a multiple of the disk sector size (typically 512 or 4096).
    pub unbuffered_buffer_bytes: usize,

    /// Buffer size for buffered I/O copies.
    pub buffered_buffer_bytes: usize,

    /// Number of worker threads for parallel small-file copies.
    pub thread_count: usize,

    /// Maximum total memory budget for copy buffers across all threads (bytes).
    pub max_memory_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        let cpus = num_cpus::get().max(1);
        // Cap threads between 2 and 8 by default.
        let threads = cpus.clamp(2, 8);
        Self {
            // 16 MiB default threshold before any benchmark runs.
            size_threshold_bytes: 16 * 1024 * 1024,
            // 8 MiB unbuffered buffer -- a common sweet spot for sequential reads.
            unbuffered_buffer_bytes: 8 * 1024 * 1024,
            // 1 MiB buffered buffer.
            buffered_buffer_bytes: 1024 * 1024,
            thread_count: threads,
            // 512 MiB memory ceiling.
            max_memory_bytes: 512 * 1024 * 1024,
        }
    }
}

impl Config {
    /// Resolve the path to the config file (next to the running executable).
    pub fn config_path() -> Option<PathBuf> {
        std::env::current_exe().ok().and_then(|exe| {
            exe.parent().map(|dir| dir.join("fast-copy.json"))
        })
    }

    /// Load config from disk, falling back to defaults on any error.
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|p| Self::load_from(&p).ok())
            .unwrap_or_default()
    }

    /// Load from a specific path.
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse config: {}", e))
    }

    /// Persist the current config to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path()
            .ok_or_else(|| "Cannot determine config path".to_string())?;
        self.save_to(&path)
    }

    /// Save to a specific path.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        std::fs::write(path, data)
            .map_err(|e| format!("Failed to write config: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_default_config_values() {
        let cfg = Config::default();
        assert_eq!(cfg.size_threshold_bytes, 16 * 1024 * 1024);
        assert_eq!(cfg.unbuffered_buffer_bytes, 8 * 1024 * 1024);
        assert_eq!(cfg.buffered_buffer_bytes, 1024 * 1024);
        assert!(cfg.thread_count >= 2 && cfg.thread_count <= 8);
    }

    #[test]
    fn test_round_trip_serialization() {
        let cfg = Config::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let loaded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.size_threshold_bytes, cfg.size_threshold_bytes);
        assert_eq!(loaded.thread_count, cfg.thread_count);
    }

    #[test]
    fn test_load_from_file() {
        let dir = std::env::temp_dir().join("fast_copy_test_config");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.json");

        let mut cfg = Config::default();
        cfg.size_threshold_bytes = 42;
        cfg.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.size_threshold_bytes, 42);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_from_missing_file() {
        let result = Config::load_from(Path::new("/nonexistent/path.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_invalid_json() {
        let dir = std::env::temp_dir().join("fast_copy_test_bad_json");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"not json").unwrap();

        let result = Config::load_from(&path);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
