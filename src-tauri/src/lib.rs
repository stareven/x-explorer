mod android_client;
mod bin_path;
mod device_manager;
mod file_ops;
mod ios_client;
mod transfer_queue;
mod types;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri_plugin_log::{Target, TargetKind};

    // Debug builds (`tauri dev`) log at Debug level to stdout; release builds
    // (`tauri build`) log at Info level to a file in the OS logs directory.
    let log_plugin = if cfg!(debug_assertions) {
        tauri_plugin_log::Builder::new()
            .level(log::LevelFilter::Debug)
            .targets([Target::new(TargetKind::Stdout)])
            .build()
    } else {
        tauri_plugin_log::Builder::new()
            .level(log::LevelFilter::Info)
            .targets([Target::new(TargetKind::LogDir {
                file_name: Some("x-explorer.log".into()),
            })])
            .build()
    };

    tauri::Builder::default()
        .plugin(log_plugin)
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
            ios_client::enqueue_ios_file_info,
            ios_client::ios_delete,
            android_client::list_android_devices,
            android_client::list_android_apps,
            android_client::list_android_files,
            android_client::android_delete,
            transfer_queue::cancel_transfer,
            transfer_queue::enqueue_ios_download,
            transfer_queue::enqueue_ios_upload,
            transfer_queue::enqueue_ios_delete,
            transfer_queue::enqueue_ios_download_batch,
            transfer_queue::enqueue_ios_upload_batch,
            transfer_queue::enqueue_ios_delete_batch,
            transfer_queue::enqueue_android_download,
            transfer_queue::enqueue_android_upload,
            transfer_queue::enqueue_android_delete,
            transfer_queue::enqueue_android_download_batch,
            transfer_queue::enqueue_android_upload_batch,
            transfer_queue::enqueue_android_delete_batch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
