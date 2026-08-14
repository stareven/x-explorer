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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTask {
    pub id: String,
    pub kind: String, // "upload" | "download"
    pub src: String,
    pub dst: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: String, // "pending" | "running" | "done" | "error" | "cancelled"
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub task_id: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub status: String,
}
