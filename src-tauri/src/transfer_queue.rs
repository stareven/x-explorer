//! Single execution path for all uploads/downloads. Wraps the plain `*_download`/
//! `*_upload` functions in `ios_client`/`android_client` behind a queue so the
//! frontend only ever reaches transfer execution through `enqueue_*` commands.
//!
//! STUB: This module is a placeholder created in Task 1 so the project compiles
//! standalone and the Tauri commands referenced by `main.rs` resolve. The real
//! queue implementation is added in Task 6.

#[tauri::command]
pub fn cancel_transfer(_task_id: String) -> Result<(), String> {
    Err("not implemented".into())
}

#[tauri::command]
pub fn enqueue_ios_download(
    _device_id: String,
    _bundle_id: String,
    _src: String,
    _dst: String,
) -> Result<String, String> {
    Err("not implemented".into())
}

#[tauri::command]
pub fn enqueue_ios_upload(
    _device_id: String,
    _bundle_id: String,
    _src: String,
    _dst: String,
) -> Result<String, String> {
    Err("not implemented".into())
}

#[tauri::command]
pub fn enqueue_android_download(
    _device_id: String,
    _package: String,
    _src: String,
    _dst: String,
) -> Result<String, String> {
    Err("not implemented".into())
}

#[tauri::command]
pub fn enqueue_android_upload(
    _device_id: String,
    _package: String,
    _src: String,
    _dst: String,
) -> Result<String, String> {
    Err("not implemented".into())
}
