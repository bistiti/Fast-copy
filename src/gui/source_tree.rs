// Source tree: data model for the left panel showing added source files/folders.
// Each entry has a checkbox (included/excluded). Toggling a directory
// propagates to all its children.

use std::path::PathBuf;

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

    /// Create a directory node by scanning its contents recursively.
    /// Errors during scanning are silently skipped (the directory will appear empty).
    pub fn directory(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let mut children = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&path) {
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            // Sort: directories first, then alphabetical.
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

            for entry in entries {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    children.push(SourceNode::directory(entry_path));
                } else {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    children.push(SourceNode::file(entry_path, size));
                }
            }
        }

        Self {
            name,
            path,
            is_dir: true,
            included: true,
            children,
            size: 0,
        }
    }

    /// Set the included state for this node and all descendants.
    pub fn set_included_recursive(&mut self, included: bool) {
        self.included = included;
        for child in &mut self.children {
            child.set_included_recursive(included);
        }
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

    /// Add a directory (recursively scanned) to the source list.
    pub fn add_directory(&mut self, path: PathBuf) {
        self.roots.push(SourceNode::directory(path));
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

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
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
}
