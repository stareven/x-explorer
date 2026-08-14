//! Talks to libimobiledevice-family binaries to enumerate iOS devices, apps and files.
//!
//! STUB: This module is a placeholder created in Task 1 so the project compiles
//! standalone and the Tauri commands referenced by `main.rs` resolve. The real
//! implementation (and the `ios_download`/`ios_upload` plain functions used
//! internally by `transfer_queue`) is added in Task 4.

use crate::types::{AppInfo, Device, FileEntry};

#[tauri::command]
pub fn list_ios_devices() -> Result<Vec<Device>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn list_ios_apps(_device_id: String) -> Result<Vec<AppInfo>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn list_ios_files(_device_id: String, _bundle_id: String, _path: String) -> Result<Vec<FileEntry>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn ios_delete(_device_id: String, _bundle_id: String, _path: String) -> Result<(), String> {
    Err("not implemented".into())
}

#[tauri::command]
pub fn ios_unmount_container(_device_id: String, _bundle_id: String) -> Result<(), String> {
    Err("not implemented".into())
}

// NOT registered as tauri::command — called internally by transfer_queue (Task 6).
pub fn ios_download(
    _device_id: String,
    _bundle_id: String,
    _src: String,
    _dst: String,
) -> Result<(), String> {
    Err("not implemented".into())
}

// NOT registered as tauri::command — called internally by transfer_queue (Task 6).
pub fn ios_upload(
    _device_id: String,
    _bundle_id: String,
    _src: String,
    _dst: String,
) -> Result<(), String> {
    Err("not implemented".into())
}
