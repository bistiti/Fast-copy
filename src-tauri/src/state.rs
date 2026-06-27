// Application state managed by Tauri and shared across command invocations.

use crate::config::Config;
use crate::engine::worker::CopyControl;
use crate::sources::SourceList;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Global application state. All fields are behind individual mutexes so that
/// unrelated commands don't contend on a single lock.
pub struct AppState {
    /// User-tunable configuration (also persisted to disk).
    pub config: Mutex<Config>,
    /// The source tree (files/folders the user has added).
    pub sources: Mutex<SourceList>,
    /// Selected destination directory.
    pub destination: Mutex<Option<PathBuf>>,
    /// Control handle for the currently-running copy (pause/resume/cancel).
    pub copy_control: Mutex<Option<Arc<CopyControl>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Config::load()),
            sources: Mutex::new(SourceList::new()),
            destination: Mutex::new(None),
            copy_control: Mutex::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
