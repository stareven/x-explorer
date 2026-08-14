//! Talks to adb to enumerate Android devices, apps and files.
//!
//! STUB: This module is a placeholder created in Task 1 so the project compiles
//! standalone and the Tauri commands referenced by `main.rs` resolve. The real
//! implementation (and the `android_download`/`android_upload` plain functions
//! used internally by `transfer_queue`) is added in Task 3.

use crate::types::{AppInfo, Device, FileEntry};

#[tauri::command]
pub fn list_android_devices() -> Result<Vec<Device>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn list_android_apps(_device_id: String) -> Result<Vec<AppInfo>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn list_android_files(_device_id: String, _package: String, _path: String) -> Result<Vec<FileEntry>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub fn android_delete(_device_id: String, _package: String, _path: String) -> Result<(), String> {
    Err("not implemented".into())
}

// NOT registered as tauri::command — called internally by transfer_queue (Task 6).
pub fn android_download(
    _device_id: String,
    _package: String,
    _src: String,
    _dst: String,
) -> Result<(), String> {
    Err("not implemented".into())
}

// NOT registered as tauri::command — called internally by transfer_queue (Task 6).
pub fn android_upload(
    _device_id: String,
    _package: String,
    _src: String,
    _dst: String,
) -> Result<(), String> {
    Err("not implemented".into())
}
