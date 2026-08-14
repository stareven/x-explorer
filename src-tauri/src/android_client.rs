//! Talks to adb to enumerate Android devices, apps and files.
//!
//! Android has two distinct access modes that must not be confused:
//! - External storage (`/sdcard/...`): plain `adb shell ls`, `adb pull`, `adb push` —
//!   no special permission needed.
//! - App container (`/data/data/<pkg>/...`): SELinux blocks direct access. Every
//!   operation must go through `run-as <pkg> <cmd>`. Uploads must stage through
//!   `/data/local/tmp` (world-writable) because `run-as` has no direct way to
//!   receive a pushed file. If the target app is not debuggable, `run-as` fails
//!   with a message containing "not debuggable" — this is surfaced as a distinct,
//!   user-facing error rather than a generic failure.
//!
//! Every command below also guards against running further `adb shell` calls
//! against an unauthorized device via `check_device_authorized`, since `adb
//! devices` reports `unauthorized` for devices where USB debugging hasn't been
//! approved on-device yet, and shelling out anyway just produces a confusing raw
//! error.

use crate::types::{AppInfo, Device, FileEntry};

/// Parse output of `adb devices` into a list of Device structs.
pub fn parse_adb_devices(output: &str) -> Vec<Device> {
    output
        .lines()
        .skip(1) // skip "List of devices attached" header
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            let id = parts[0].trim().to_string();
            let status = if parts.len() > 1 {
                match parts[1].trim() {
                    "device" => "connected",
                    "unauthorized" => "unauthorized",
                    _ => "offline",
                }
                .to_string()
            } else {
                "offline".to_string()
            };
            Device {
                id: id.clone(),
                name: id,
                platform: "android".to_string(),
                status,
            }
        })
        .collect()
}

/// Parse output of `adb shell pm list packages` into AppInfo list.
pub fn parse_adb_packages(output: &str) -> Vec<AppInfo> {
    output
        .lines()
        .filter(|line| line.starts_with("package:"))
        .map(|line| {
            let bundle_id = line["package:".len()..].trim().to_string();
            AppInfo {
                name: bundle_id.clone(),
                bundle_id,
            }
        })
        .collect()
}

/// Parse output of `ls -la <path>` (works for both plain adb shell and run-as) into FileEntry list.
pub fn parse_adb_ls(output: &str, base_path: &str) -> Vec<FileEntry> {
    output
        .lines()
        .filter(|line| !line.starts_with("total") && !line.trim().is_empty())
        .filter_map(|line| {
            // Format: permissions links owner group size date time name
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 8 {
                return None;
            }
            let is_dir = parts[0].starts_with('d');
            let size: u64 = parts[4].parse().unwrap_or(0);
            let name = parts[7..].join(" ");
            if name == "." || name == ".." {
                return None;
            }
            Some(FileEntry {
                path: format!("{}/{}", base_path.trim_end_matches('/'), name),
                name,
                is_dir,
                size,
                modified: None,
            })
        })
        .collect()
}

/// Returns true if a run-as invocation failed because the target app is not debuggable.
pub fn is_not_debuggable_error(stderr: &str) -> bool {
    stderr.contains("not debuggable") || stderr.contains("run-as: Package")
}

/// If a run-as command's stderr indicates the target app isn't debuggable,
/// returns the user-facing error message for it. Returns None otherwise.
fn not_debuggable_error(stderr: &str) -> Option<String> {
    if is_not_debuggable_error(stderr) {
        Some("该应用未开启调试模式，无法访问其数据目录".to_string())
    } else {
        None
    }
}

/// Builds the adb shell argv for a command, wrapping it in `run-as <pkg>` when
/// `package` is Some, or running it plain when None. Returns the args that come
/// after "-s" "<device_id>" "shell".
fn shell_args(package: &Option<String>, cmd: &[&str]) -> Vec<String> {
    match package {
        Some(pkg) => {
            let mut v = vec!["run-as".to_string(), pkg.clone()];
            v.extend(cmd.iter().map(|s| s.to_string()));
            v
        }
        None => cmd.iter().map(|s| s.to_string()).collect(),
    }
}

/// Whether a path falls under the app-container namespace and therefore needs run-as.
pub fn requires_run_as(path: &str) -> bool {
    path.starts_with("/data/data/") || path.starts_with("/data/user/")
}

/// Checks the device's current status via `adb devices` and returns an error
/// if it isn't `connected`. Every command that shells further into the device
/// (ls/cat/push/pull/rm) calls this first, so an unauthorized/offline device
/// produces one clear error message instead of a raw, confusing adb stderr.
pub fn check_device_authorized(device_id: &str) -> Result<(), String> {
    let adb = crate::bin_path::resolve("adb")?;
    let out = std::process::Command::new(&adb)
        .arg("devices")
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let devices = parse_adb_devices(&text);
    match devices.iter().find(|d| d.id == device_id) {
        Some(d) if d.status == "connected" => Ok(()),
        Some(d) if d.status == "unauthorized" => {
            Err("设备未授权，请在手机上允许 USB 调试".to_string())
        }
        Some(_) => Err("设备已离线".to_string()),
        None => Err("设备未连接".to_string()),
    }
}

#[tauri::command]
pub fn list_android_devices() -> Result<Vec<Device>, String> {
    let adb = crate::bin_path::resolve("adb")?;
    let out = std::process::Command::new(adb)
        .arg("devices")
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(parse_adb_devices(&text))
}

#[tauri::command]
pub fn list_android_apps(device_id: String) -> Result<Vec<AppInfo>, String> {
    check_device_authorized(&device_id)?;
    let adb = crate::bin_path::resolve("adb")?;
    let out = std::process::Command::new(adb)
        .args(["-s", &device_id, "shell", "pm", "list", "packages"])
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(parse_adb_packages(&text))
}

/// Returns the app-container root path for a given package name.
pub fn pkg_root(pkg: &str) -> String {
    format!("/data/data/{}", pkg)
}

/// List files. `package` is None for external storage browsing (path is an absolute
/// /sdcard/... path), Some(pkg) for app-container browsing (path is relative to the
/// package's data root, e.g. "/" or "/shared_prefs").
#[tauri::command]
pub fn list_android_files(
    device_id: String,
    path: String,
    package: Option<String>,
) -> Result<Vec<FileEntry>, String> {
    check_device_authorized(&device_id)?;
    let adb = crate::bin_path::resolve("adb")?;
    let full_path = match &package {
        Some(pkg) => crate::file_ops::join_path(&pkg_root(pkg), &path),
        None => crate::file_ops::normalize_path(&path),
    };
    let mut args: Vec<String> = vec!["-s".to_string(), device_id.clone(), "shell".to_string()];
    args.extend(shell_args(&package, &["ls", "-la", &full_path]));
    let out = std::process::Command::new(adb)
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(err) = not_debuggable_error(&stderr) {
        return Err(err);
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(parse_adb_ls(&text, &full_path))
}

/// Download a single file. External storage uses `adb pull` directly.
/// App-container files are streamed via `run-as <pkg> cat <path>` piped to a local file,
/// since `adb pull` cannot read paths gated by SELinux.
/// Not a #[tauri::command] — called internally by transfer_queue, which is the
/// only path the frontend uses to trigger downloads (see Task 1's main.rs note).
pub fn android_download(
    device_id: String,
    remote_path: String,
    local_path: String,
    package: Option<String>,
) -> Result<(), String> {
    check_device_authorized(&device_id)?;
    let adb = crate::bin_path::resolve("adb")?;
    let remote_path = match &package {
        Some(pkg) => crate::file_ops::join_path(&pkg_root(pkg), &remote_path),
        None => crate::file_ops::normalize_path(&remote_path),
    };
    match package {
        None => {
            let status = std::process::Command::new(&adb)
                .args(["-s", &device_id, "pull", &remote_path, &local_path])
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() { Ok(()) } else { Err("adb pull failed".to_string()) }
        }
        Some(pkg) => {
            let mut args: Vec<String> = vec!["-s".to_string(), device_id.clone(), "shell".to_string()];
            args.extend(shell_args(&Some(pkg), &["cat", &remote_path]));
            let out = std::process::Command::new(&adb)
                .args(&args)
                .output()
                .map_err(|e| e.to_string())?;
            let stderr = String::from_utf8_lossy(&out.stderr);
            if let Some(err) = not_debuggable_error(&stderr) {
                return Err(err);
            }
            std::fs::write(&local_path, &out.stdout).map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

/// Upload a single file. External storage uses `adb push` directly.
/// App-container files are staged via `/data/local/tmp` (world-writable), then moved in with
/// `run-as <pkg> cp`, then the staged copy is removed.
/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn android_upload(
    device_id: String,
    local_path: String,
    remote_path: String,
    package: Option<String>,
) -> Result<(), String> {
    check_device_authorized(&device_id)?;
    let adb = crate::bin_path::resolve("adb")?;
    let remote_path = match &package {
        Some(pkg) => crate::file_ops::join_path(&pkg_root(pkg), &remote_path),
        None => crate::file_ops::normalize_path(&remote_path),
    };
    match package {
        None => {
            let status = std::process::Command::new(&adb)
                .args(["-s", &device_id, "push", &local_path, &remote_path])
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() { Ok(()) } else { Err("adb push failed".to_string()) }
        }
        Some(pkg) => {
            let tmp_name = format!(
                "x-explorer-{}",
                std::path::Path::new(&local_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "upload".to_string())
            );
            let staging_path = format!("/data/local/tmp/{}", tmp_name);

            let push_status = std::process::Command::new(&adb)
                .args(["-s", &device_id, "push", &local_path, &staging_path])
                .status()
                .map_err(|e| e.to_string())?;
            if !push_status.success() {
                return Err("adb push to staging area failed".to_string());
            }

            let mut cp_args: Vec<String> = vec!["-s".to_string(), device_id.clone(), "shell".to_string()];
            cp_args.extend(shell_args(&Some(pkg.clone()), &["cp", &staging_path, &remote_path]));
            let cp_out = std::process::Command::new(&adb)
                .args(&cp_args)
                .output()
                .map_err(|e| e.to_string())?;
            let stderr = String::from_utf8_lossy(&cp_out.stderr);
            let not_debuggable_err = not_debuggable_error(&stderr);

            // Always clean up the staged file regardless of cp outcome.
            let _ = std::process::Command::new(&adb)
                .args(["-s", &device_id, "shell", "rm", "-f", &staging_path])
                .status();

            if let Some(err) = not_debuggable_err {
                return Err(err);
            }
            if !cp_out.status.success() {
                return Err("run-as cp failed".to_string());
            }
            Ok(())
        }
    }
}

#[tauri::command]
pub fn android_delete(
    device_id: String,
    remote_path: String,
    package: Option<String>,
) -> Result<(), String> {
    check_device_authorized(&device_id)?;
    let adb = crate::bin_path::resolve("adb")?;
    let remote_path = match &package {
        Some(pkg) => crate::file_ops::join_path(&pkg_root(pkg), &remote_path),
        None => crate::file_ops::normalize_path(&remote_path),
    };
    let mut args: Vec<String> = vec!["-s".to_string(), device_id.clone(), "shell".to_string()];
    args.extend(shell_args(&package, &["rm", "-rf", &remote_path]));
    let out = std::process::Command::new(&adb)
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(err) = not_debuggable_error(&stderr) {
        return Err(err);
    }
    if out.status.success() { Ok(()) } else { Err("rm failed".to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_adb_devices_connected() {
        let output = "List of devices attached\nemulator-5554\tdevice\n";
        let devices = parse_adb_devices(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "emulator-5554");
        assert_eq!(devices[0].status, "connected");
        assert_eq!(devices[0].platform, "android");
    }

    #[test]
    fn test_parse_adb_devices_unauthorized() {
        let output = "List of devices attached\nABCD1234\tunauthorized\n";
        let devices = parse_adb_devices(output);
        assert_eq!(devices[0].status, "unauthorized");
    }

    #[test]
    fn test_parse_adb_devices_empty() {
        let output = "List of devices attached\n\n";
        let devices = parse_adb_devices(output);
        assert_eq!(devices.len(), 0);
    }

    #[test]
    fn test_parse_adb_packages() {
        let output = "package:com.example.app\npackage:com.android.settings\n";
        let apps = parse_adb_packages(output);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].bundle_id, "com.example.app");
    }

    #[test]
    fn test_parse_adb_ls() {
        let output = "-rw-rw---- 1 root sdcard_rw 1234 2024-01-01 12:00 test.txt\ndrwxr-xr-x 2 root root 4096 2024-01-01 12:00 images\n";
        let entries = parse_adb_ls(output, "/sdcard");
        assert_eq!(entries.len(), 2);
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].name, "test.txt");
        assert_eq!(entries[0].size, 1234);
        assert!(entries[1].is_dir);
        assert_eq!(entries[1].name, "images");
    }

    #[test]
    fn test_parse_adb_ls_skips_dot_entries() {
        let output = "drwxr-xr-x 2 root root 4096 2024-01-01 12:00 .\ndrwxr-xr-x 2 root root 4096 2024-01-01 12:00 ..\n-rw-r--r-- 1 root root 100 2024-01-01 12:00 file.txt\n";
        let entries = parse_adb_ls(output, "/sdcard");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
    }

    #[test]
    fn test_is_not_debuggable_error_detects_run_as_failure() {
        assert!(is_not_debuggable_error("run-as: Package 'com.example.app' is not debuggable\n"));
        assert!(!is_not_debuggable_error("total 24\n"));
    }

    #[test]
    fn test_requires_run_as_for_app_data_paths() {
        assert!(requires_run_as("/data/data/com.example.app/files"));
        assert!(requires_run_as("/data/user/0/com.example.app/files"));
        assert!(!requires_run_as("/sdcard/DCIM"));
        assert!(!requires_run_as("/storage/emulated/0/Download"));
    }

    #[test]
    fn test_pkg_root_format() {
        assert_eq!(pkg_root("com.example.app"), "/data/data/com.example.app");
    }

    #[test]
    fn test_shell_args_wraps_with_run_as_when_package_present() {
        let package = Some("com.example.app".to_string());
        let args = shell_args(&package, &["ls", "-la", "/data/data/com.example.app"]);
        assert_eq!(
            args,
            vec![
                "run-as".to_string(),
                "com.example.app".to_string(),
                "ls".to_string(),
                "-la".to_string(),
                "/data/data/com.example.app".to_string(),
            ]
        );
    }

    #[test]
    fn test_shell_args_plain_when_no_package() {
        let package: Option<String> = None;
        let args = shell_args(&package, &["ls", "-la", "/sdcard"]);
        assert_eq!(
            args,
            vec!["ls".to_string(), "-la".to_string(), "/sdcard".to_string()]
        );
    }

    #[test]
    fn test_not_debuggable_error_returns_message_when_detected() {
        let err = not_debuggable_error("run-as: Package 'com.example.app' is not debuggable\n");
        assert_eq!(err, Some("该应用未开启调试模式，无法访问其数据目录".to_string()));
    }

    #[test]
    fn test_not_debuggable_error_returns_none_when_not_detected() {
        assert_eq!(not_debuggable_error("total 24\n"), None);
    }
}
