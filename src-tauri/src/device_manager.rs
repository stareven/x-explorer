//! Watches for connected iOS/Android devices and emits device-list events to the frontend.
//!
//! Polls both platforms on a fixed interval on a background thread and emits
//! `devices-changed` to the frontend only when the combined device list actually
//! changes (by count or by id/status), rather than on every tick.

use crate::types::Device;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn start(handle: AppHandle) {
    let known: Arc<Mutex<Vec<Device>>> = Arc::new(Mutex::new(Vec::new()));
    thread::spawn(move || loop {
        let mut all_devices: Vec<Device> = Vec::new();

        // iOS — reuse list_ios_devices so trust-state detection stays in one place.
        if let Ok(ios) = crate::ios_client::list_ios_devices() {
            all_devices.extend(ios);
        }

        // Android
        if let Ok(out) = run_silent("adb", &["devices"]) {
            let android = crate::android_client::parse_adb_devices(&out);
            all_devices.extend(android);
        }

        let mut known_lock = known.lock().unwrap();
        let changed = all_devices.len() != known_lock.len()
            || all_devices
                .iter()
                .any(|d| !known_lock.iter().any(|k| k.id == d.id && k.status == d.status));

        if changed {
            *known_lock = all_devices.clone();
            let _ = handle.emit("devices-changed", all_devices);
        }
        drop(known_lock);

        thread::sleep(Duration::from_secs(2));
    });
}

fn run_silent(name: &str, args: &[&str]) -> Result<String, String> {
    let bin = crate::bin_path::resolve(name)?;
    let out = std::process::Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
