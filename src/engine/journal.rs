// Copy journal: tracks which files have been successfully copied so that
// an interrupted copy session can be resumed without re-copying finished files.
//
// The journal is a simple newline-delimited text file. Each line is the
// absolute destination path of a file that completed successfully.
// On resume, any file whose destination appears in the journal is skipped.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Tracks completed file copies for resume support.
#[derive(Debug)]
pub struct CopyJournal {
    /// Path to the journal file on disk.
    path: PathBuf,
    /// Set of destination paths that have been completed.
    completed: HashSet<String>,
}

impl CopyJournal {
    /// Create or open a journal file. If the file exists, load its contents.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let mut journal = Self {
            path,
            completed: HashSet::new(),
        };
        journal.load()?;
        Ok(journal)
    }

    /// Determine the default journal path (next to the executable, or %TEMP%).
    pub fn default_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join("fast-copy-journal.log")))
            .unwrap_or_else(|| {
                std::env::temp_dir().join("fast-copy-journal.log")
            })
    }

    /// Load existing entries from the journal file. Missing file is not an error.
    fn load(&mut self) -> Result<(), String> {
        match std::fs::read_to_string(&self.path) {
            Ok(contents) => {
                for line in contents.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        self.completed.insert(trimmed.to_string());
                    }
                }
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Failed to read journal at {:?}: {}", self.path, e)),
        }
    }

    /// Record a successfully copied file (by its destination path).
    /// Appends immediately to disk so progress is durable.
    pub fn record_completed(&mut self, destination: &Path) -> Result<(), String> {
        let key = destination.to_string_lossy().to_string();
        if self.completed.insert(key.clone()) {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .map_err(|e| format!("Failed to open journal for writing: {}", e))?;
            writeln!(file, "{}", key)
                .map_err(|e| format!("Failed to write journal entry: {}", e))?;
        }
        Ok(())
    }

    /// Check whether a destination path has already been completed.
    pub fn is_completed(&self, destination: &Path) -> bool {
        self.completed.contains(&destination.to_string_lossy().to_string())
    }

    /// Clear the journal (e.g., when starting a fresh copy session).
    #[allow(dead_code)]
    pub fn clear(&mut self) -> Result<(), String> {
        self.completed.clear();
        if self.path.exists() {
            std::fs::remove_file(&self.path)
                .map_err(|e| format!("Failed to remove journal: {}", e))?;
        }
        Ok(())
    }

    /// Number of completed entries.
    #[allow(dead_code)]
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU32, Ordering};
    static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_journal() -> (PathBuf, CopyJournal) {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join("fast_copy_journal_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("test_{}_{}.log", std::process::id(), id));
        let _ = std::fs::remove_file(&path);
        let journal = CopyJournal::open(path.clone()).unwrap();
        (path, journal)
    }

    #[test]
    fn test_empty_journal() {
        let (_path, journal) = temp_journal();
        assert_eq!(journal.completed_count(), 0);
        assert!(!journal.is_completed(Path::new("C:\\foo\\bar.txt")));
    }

    #[test]
    fn test_record_and_check() {
        let (path, mut journal) = temp_journal();
        let dest = Path::new("C:\\dest\\file.txt");

        journal.record_completed(dest).unwrap();
        assert!(journal.is_completed(dest));
        assert_eq!(journal.completed_count(), 1);

        // Duplicate recording should not increase count.
        journal.record_completed(dest).unwrap();
        assert_eq!(journal.completed_count(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_persistence_across_loads() {
        let (path, mut journal) = temp_journal();
        let dest = Path::new("C:\\persist\\test.dat");

        journal.record_completed(dest).unwrap();
        drop(journal);

        let journal2 = CopyJournal::open(path.clone()).unwrap();
        assert!(journal2.is_completed(dest));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_clear() {
        let (path, mut journal) = temp_journal();
        journal.record_completed(Path::new("C:\\a.txt")).unwrap();
        journal.clear().unwrap();
        assert_eq!(journal.completed_count(), 0);
        assert!(!path.exists());
    }
}
