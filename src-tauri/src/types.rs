use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: String, // "ios" | "android"
    pub status: String,   // "connected" | "unauthorized" | "offline"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub bundle_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>, // unix timestamp
}

#[derive(Debug, Clone)]
pub struct DownloadFile {
    pub remote_path: String,
    pub local_path: String,
}

/// A single leaf produced by `collect_ios_delete_targets_recursive`. Either a
/// file (will be `rm`'d) or an empty subdirectory (will be `rmdir`'d in the
/// post-pass after its contents are gone). Paths are user-facing relative
/// paths under `/Documents`, already sanitized (no `..`).
#[derive(Clone, Debug)]
pub struct IosDeleteTarget {
    pub remote_path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTask {
    pub id: String,
    pub kind: String, // "upload" | "download" | "delete"
    pub src: String,
    pub dst: String,
    pub total_files: u64,
    pub completed_files: u64,
    pub status: String, // "pending" | "running" | "done" | "error" | "cancelled"
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub task_id: String,
    pub kind: String,
    pub src: String,
    pub dst: String,
    pub total_files: u64,
    pub completed_files: u64,
    pub status: String,
    pub error: Option<String>,
}

/// Emitted once per file after `enqueue_ios_file_info` finishes probing it via
/// `afcclient info`. `path` matches the `FileEntry.path` the frontend already
/// has, so it can be looked up and patched in place.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosFileInfoReady {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
}
