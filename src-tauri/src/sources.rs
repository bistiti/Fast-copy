// Source tree: data model for the source list showing added files/folders.
// Each entry has a checkbox (included/excluded). Toggling a directory
// propagates to all its children.

use crate::scan_progress::ScanProgress;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// A node in the source tree. Can be a file or a directory with children.
#[derive(Debug, Clone)]
pub struct SourceNode {
    /// Display name (file/directory name, not full path).
    pub name: String,
    /// Full absolute path.
    pub path: PathBuf,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Whether this item is included in the copy (checkbox state).
    pub included: bool,
    /// Children (only populated for directories).
    pub children: Vec<SourceNode>,
    /// File size in bytes (0 for directories).
    pub size: u64,
}

impl SourceNode {
    /// Create a file node.
    pub fn file(path: PathBuf, size: u64) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        Self {
            name,
            path,
            is_dir: false,
            included: true,
            children: Vec::new(),
            size,
        }
    }

    /// Set the included state for this node and all descendants.
    pub fn set_included_recursive(&mut self, included: bool) {
        self.included = included;
        for child in &mut self.children {
            child.set_included_recursive(included);
        }
    }

    /// Find the node whose path matches `target` and set its included state
    /// (cascading to descendants). Returns true if a matching node was found.
    pub fn set_included_for_path(&mut self, target: &Path, included: bool) -> bool {
        if self.path == target {
            self.set_included_recursive(included);
            return true;
        }
        for child in &mut self.children {
            if child.set_included_for_path(target, included) {
                return true;
            }
        }
        false
    }

    /// Count total included files (recursively).
    pub fn count_included_files(&self) -> usize {
        if !self.included {
            return 0;
        }
        if !self.is_dir {
            return 1;
        }
        self.children.iter().map(|c| c.count_included_files()).sum()
    }

    /// Sum total size of included files (recursively).
    pub fn total_included_size(&self) -> u64 {
        if !self.included {
            return 0;
        }
        if !self.is_dir {
            return self.size;
        }
        self.children.iter().map(|c| c.total_included_size()).sum()
    }

    /// Collect all included file paths with their sizes (for building the copy queue).
    pub fn collect_included_files(&self) -> Vec<(PathBuf, u64)> {
        let mut result = Vec::new();
        self.collect_files_inner(&mut result);
        result
    }

    fn collect_files_inner(&self, out: &mut Vec<(PathBuf, u64)>) {
        if !self.included {
            return;
        }
        if !self.is_dir {
            out.push((self.path.clone(), self.size));
        } else {
            for child in &self.children {
                child.collect_files_inner(out);
            }
        }
    }
}

/// The overall source list: a flat list of top-level nodes added by the user.
#[derive(Debug, Clone, Default)]
pub struct SourceList {
    pub roots: Vec<SourceNode>,
}

impl SourceList {
    pub fn new() -> Self {
        Self { roots: Vec::new() }
    }

    /// Add a file to the source list.
    pub fn add_file(&mut self, path: PathBuf) {
        if let Ok(meta) = std::fs::metadata(&path) {
            self.roots.push(SourceNode::file(path, meta.len()));
        }
    }

    /// Total included files across all roots.
    pub fn total_included_files(&self) -> usize {
        self.roots.iter().map(|r| r.count_included_files()).sum()
    }

    /// Total size of included files across all roots.
    pub fn total_included_size(&self) -> u64 {
        self.roots.iter().map(|r| r.total_included_size()).sum()
    }

    /// Collect all included files.
    pub fn collect_all_included(&self) -> Vec<(PathBuf, u64)> {
        let mut result = Vec::new();
        for root in &self.roots {
            result.extend(root.collect_included_files());
        }
        result
    }

    /// Remove a root by index.
    pub fn remove_root(&mut self, index: usize) {
        if index < self.roots.len() {
            self.roots.remove(index);
        }
    }

    /// Set the included state of the node identified by `target` path.
    pub fn set_included_for_path(&mut self, target: &Path, included: bool) {
        for root in &mut self.roots {
            if root.set_included_for_path(target, included) {
                break;
            }
        }
    }

    /// Remove all roots.
    pub fn clear(&mut self) {
        self.roots.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

/// Count the immediate subdirectories of `path` (one cheap read, no recursion).
/// Used to seed the scan ETA's top-level total (T).
pub fn count_top_level_subdirs(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count() as u64
        })
        .unwrap_or(0)
}

/// Read a directory's entries, sorted directories-first then case-insensitively.
fn sorted_entries(path: &Path) -> Vec<std::fs::DirEntry> {
    match std::fs::read_dir(path) {
        Ok(rd) => {
            let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
            entries.sort_by(|a, b| {
                let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
                b_dir.cmp(&a_dir).then_with(|| {
                    a.file_name()
                        .to_string_lossy()
                        .to_lowercase()
                        .cmp(&b.file_name().to_string_lossy().to_lowercase())
                })
            });
            entries
        }
        Err(_) => Vec::new(),
    }
}

/// Recursively scan a directory subtree, bumping `progress` per entry. Returns
/// `None` if cancelled at any depth. Does NOT touch the top-level (T/C) counters.
fn scan_subtree(path: PathBuf, cancel: &AtomicBool, progress: &ScanProgress) -> Option<SourceNode> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    progress.add_folder(&path.to_string_lossy());

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let mut children = Vec::new();
    for entry in sorted_entries(&path) {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let entry_path = entry.path();
        if entry_path.is_dir() {
            children.push(scan_subtree(entry_path, cancel, progress)?);
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            progress.add_file(size);
            children.push(SourceNode::file(entry_path, size));
        }
    }

    Some(SourceNode {
        name,
        path,
        is_dir: true,
        included: true,
        children,
        size: 0,
    })
}

/// Scan a directory, treating its immediate subfolders as the top-level units
/// for ETA: completing each one bumps `top_level_done` (C).
fn scan_with_toplevel(
    path: PathBuf,
    cancel: &AtomicBool,
    progress: &ScanProgress,
) -> Option<SourceNode> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    progress.add_folder(&path.to_string_lossy());

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let mut children = Vec::new();
    for entry in sorted_entries(&path) {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let child = scan_subtree(entry_path, cancel, progress)?;
            progress.complete_top_level();
            children.push(child);
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            progress.add_file(size);
            children.push(SourceNode::file(entry_path, size));
        }
    }

    Some(SourceNode {
        name,
        path,
        is_dir: true,
        included: true,
        children,
        size: 0,
    })
}

/// Scan a single directory with live progress. Seeds T from the immediate
/// subfolder count. Returns `None` if cancelled.
pub fn scan_directory(
    path: PathBuf,
    cancel: &AtomicBool,
    progress: &ScanProgress,
) -> Option<SourceNode> {
    progress.set_top_level_total(count_top_level_subdirs(&path));
    scan_with_toplevel(path, cancel, progress)
}

/// Scan a mix of file and directory paths (drag-and-drop) with live progress.
/// T = total immediate subfolders across all dropped directories. Returns
/// `None` if cancelled.
pub fn scan_paths(
    paths: Vec<PathBuf>,
    cancel: &AtomicBool,
    progress: &ScanProgress,
) -> Option<Vec<SourceNode>> {
    let t: u64 = paths
        .iter()
        .filter(|p| p.is_dir())
        .map(|p| count_top_level_subdirs(p))
        .sum();
    progress.set_top_level_total(t);

    let mut nodes = Vec::new();
    for p in paths {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if p.is_dir() {
            nodes.push(scan_with_toplevel(p, cancel, progress)?);
        } else if let Ok(meta) = std::fs::metadata(&p) {
            progress.add_file(meta.len());
            nodes.push(SourceNode::file(p, meta.len()));
        }
    }
    Some(nodes)
}

/// Compute the destination path for a source file, preserving relative structure.
/// Files inside a directory root are placed under `dest/<root_name>/relative/path`;
/// files added individually are placed directly under `dest`.
pub fn compute_destination(
    src_path: &Path,
    source_list: &SourceList,
    dest_base: &Path,
) -> PathBuf {
    for root in &source_list.roots {
        if root.is_dir {
            if let Ok(relative) = src_path.strip_prefix(&root.path) {
                return dest_base.join(&root.name).join(relative);
            }
        }
    }
    let filename = src_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown"));
    dest_base.join(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_node() {
        let node = SourceNode::file(PathBuf::from("/tmp/test.txt"), 1024);
        assert!(!node.is_dir);
        assert!(node.included);
        assert_eq!(node.size, 1024);
        assert_eq!(node.count_included_files(), 1);
        assert_eq!(node.total_included_size(), 1024);
    }

    #[test]
    fn test_set_included_recursive() {
        let mut node = SourceNode {
            name: "dir".to_string(),
            path: PathBuf::from("/tmp/dir"),
            is_dir: true,
            included: true,
            children: vec![
                SourceNode::file(PathBuf::from("/tmp/dir/a.txt"), 100),
                SourceNode::file(PathBuf::from("/tmp/dir/b.txt"), 200),
            ],
            size: 0,
        };

        node.set_included_recursive(false);
        assert!(!node.included);
        assert!(!node.children[0].included);
        assert!(!node.children[1].included);
        assert_eq!(node.count_included_files(), 0);
        assert_eq!(node.total_included_size(), 0);
    }

    #[test]
    fn test_collect_included_files() {
        let node = SourceNode {
            name: "dir".to_string(),
            path: PathBuf::from("/tmp/dir"),
            is_dir: true,
            included: true,
            children: vec![
                SourceNode::file(PathBuf::from("/tmp/dir/a.txt"), 100),
                SourceNode {
                    name: "b.txt".to_string(),
                    path: PathBuf::from("/tmp/dir/b.txt"),
                    is_dir: false,
                    included: false,
                    children: vec![],
                    size: 200,
                },
            ],
            size: 0,
        };

        let files = node.collect_included_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1, 100);
    }

    #[test]
    fn test_source_list() {
        let mut list = SourceList::new();
        assert!(list.is_empty());
        assert_eq!(list.total_included_files(), 0);
    }

    // ---- Test 6: enumeration counts on a known tree ----
    #[test]
    fn enumeration_counts_match_known_tree() {
        use std::io::Write;
        use std::sync::atomic::AtomicU32;

        static N: AtomicU32 = AtomicU32::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "fast_copy_enum_{}_{}",
            std::process::id(),
            id
        ));
        let _ = std::fs::remove_dir_all(&root);

        // Known tree: 6 files, 4 folders (root, sub1, sub2, sub2/deep).
        let mk = |p: &Path| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let mut f = std::fs::File::create(p).unwrap();
            f.write_all(b"x").unwrap();
        };
        std::fs::create_dir_all(&root).unwrap();
        mk(&root.join("a.txt"));
        mk(&root.join("b.txt"));
        mk(&root.join("sub1/c.txt"));
        mk(&root.join("sub2/d.txt"));
        mk(&root.join("sub2/e.txt"));
        mk(&root.join("sub2/deep/f.txt"));

        let cancel = AtomicBool::new(false);
        let progress = ScanProgress::new();
        let node = scan_directory(root.clone(), &cancel, &progress).unwrap();

        assert_eq!(progress.files_found.load(Ordering::Relaxed), 6);
        assert_eq!(progress.folders_found.load(Ordering::Relaxed), 4);
        // Top-level subfolders (T) of root = sub1, sub2.
        assert_eq!(progress.top_level_total.load(Ordering::Relaxed), 2);
        // The returned tree agrees on the file count.
        assert_eq!(node.count_included_files(), 6);

        let _ = std::fs::remove_dir_all(&root);
    }
}
