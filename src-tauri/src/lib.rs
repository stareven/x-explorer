mod android_client;
mod bin_path;
mod device_manager;
mod file_ops;
mod ios_client;
mod transfer_queue;
mod types;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            use tauri::Manager;
            let handle = app.handle().clone();
            device_manager::start(handle);
            let queue = crate::transfer_queue::TransferQueue::new(app.handle().clone(), 3);
            app.manage(queue);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ios_client::list_ios_devices,
            ios_client::list_ios_apps,
            ios_client::list_ios_files,
            ios_client::ios_delete,
            android_client::list_android_devices,
            android_client::list_android_apps,
            android_client::list_android_files,
            android_client::android_delete,
            transfer_queue::cancel_transfer,
            transfer_queue::enqueue_ios_download,
            transfer_queue::enqueue_ios_upload,
            transfer_queue::enqueue_android_download,
            transfer_queue::enqueue_android_upload,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
