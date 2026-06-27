// Serializable data-transfer objects shared with the React frontend.
// These mirror the engine's internal types but are shaped for JSON / the UI.

use crate::engine::copy_item::CopyMode;
use crate::sources::{SourceList, SourceNode};
use serde::Serialize;

/// One node in the serialized source tree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeDto {
    /// Stable identifier (the absolute path as a string).
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub included: bool,
    pub size: u64,
    pub children: Vec<TreeNodeDto>,
}

impl TreeNodeDto {
    fn from_node(node: &SourceNode) -> Self {
        Self {
            id: node.path.to_string_lossy().to_string(),
            name: node.name.clone(),
            path: node.path.to_string_lossy().to_string(),
            is_dir: node.is_dir,
            included: node.included,
            size: node.size,
            children: node.children.iter().map(TreeNodeDto::from_node).collect(),
        }
    }
}

/// The whole source tree plus aggregate totals, returned by every
/// source-mutating command so the frontend can re-render in one shot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeDto {
    pub roots: Vec<TreeNodeDto>,
    pub total_files: usize,
    pub total_size: u64,
}

impl TreeDto {
    pub fn from_list(list: &SourceList) -> Self {
        Self {
            roots: list.roots.iter().map(TreeNodeDto::from_node).collect(),
            total_files: list.total_included_files(),
            total_size: list.total_included_size(),
        }
    }
}

/// One row in the copy queue, sent to the frontend when a copy starts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntryDto {
    pub index: usize,
    pub name: String,
    pub size: u64,
    /// "buffered" or "unbuffered".
    pub mode: String,
}

impl QueueEntryDto {
    pub fn new(index: usize, name: String, size: u64, mode: CopyMode) -> Self {
        let mode = match mode {
            CopyMode::Buffered => "buffered",
            CopyMode::Unbuffered => "unbuffered",
        }
        .to_string();
        Self {
            index,
            name,
            size,
            mode,
        }
    }
}

/// Free-space response for `set_destination`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationInfo {
    pub free_space: Option<u64>,
}
