// Copy engine module: orchestrates file copy operations, choosing between
// buffered and unbuffered I/O per file based on the configured threshold.

pub mod copy_item;
pub mod journal;
pub mod worker;
#[cfg(windows)]
pub mod win32;
#[cfg(not(windows))]
pub mod stub;

#[allow(unused_imports)]
pub use copy_item::{CopyItem, CopyMode, CopyStatus};
pub use journal::CopyJournal;
