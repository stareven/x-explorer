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

use crate::types::{AppInfo, Device, DownloadFile, FileEntry};
use std::fs;
use std::path::Path;

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
            let is_symlink = parts[0].starts_with('l');
            let is_dir = parts[0].starts_with('d') || is_symlink;
            let size: u64 = parts[4].parse().unwrap_or(0);
            let raw_name = parts[7..].join(" ");
            let name = if is_symlink {
                raw_name.split(" -> ").next().unwrap_or(&raw_name).to_string()
            } else {
                raw_name
            };
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

fn android_ls_mode(package: &Option<String>) -> &'static str {
    if package.is_none() {
        "-laL"
    } else {
        "-la"
    }
}

fn external_storage_paths(path: &str, resolved_root: &str) -> Option<(String, String)> {
    let display_path = crate::file_ops::normalize_path(path);
    let suffix = display_path.strip_prefix("/sdcard").unwrap_or("");
    let safe_suffix = crate::file_ops::sanitize_relative_path(suffix)?;
    let command_root = crate::file_ops::normalize_path(resolved_root);
    let command_path = if safe_suffix.is_empty() {
        command_root
    } else {
        crate::file_ops::join_path(&command_root, &safe_suffix)
    };
    Some((display_path, command_path))
}

fn resolve_external_storage_root(adb: &Path, device_id: &str) -> Result<String, String> {
    let mut args: Vec<String> = vec!["-s".to_string(), device_id.to_string(), "shell".to_string()];
    args.extend(shell_args(&None, &["ls", "-ld", "/sdcard"]));
    let out = std::process::Command::new(adb)
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if let Some(target) = stdout
        .lines()
        .find_map(|line| line.split_once(" -> ").map(|(_, target)| target.trim().to_string()))
    {
        return Ok(crate::file_ops::normalize_path(&target));
    }
    Ok("/sdcard".to_string())
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

#[tauri::command(async)]
pub fn list_android_devices() -> Result<Vec<Device>, String> {
    let adb = crate::bin_path::resolve("adb")?;
    let out = std::process::Command::new(adb)
        .arg("devices")
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(parse_adb_devices(&text))
}

#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn list_android_files(
    device_id: String,
    path: String,
    package: Option<String>,
) -> Result<Vec<FileEntry>, String> {
    check_device_authorized(&device_id)?;
    let adb = crate::bin_path::resolve("adb")?;
    let package_label = package.as_deref().unwrap_or("<external-storage>");
    eprintln!(
        "[android:list] device={} package={} request_path={}",
        device_id, package_label, path
    );
    let (display_path, command_path) = match &package {
        Some(pkg) => {
            let safe_path = crate::file_ops::sanitize_relative_path(&path)
                .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
            let command_path = crate::file_ops::join_path(&pkg_root(pkg), &safe_path);
            (crate::file_ops::normalize_path(&safe_path), command_path)
        }
        None => {
            let resolved_root = resolve_external_storage_root(&adb, &device_id)?;
            eprintln!("[android:list] resolved_root={}", resolved_root);
            external_storage_paths(&path, &resolved_root)
                .ok_or_else(|| "外部存储路径无效".to_string())?
        }
    };
    eprintln!(
        "[android:list] display_path={} command_path={}",
        display_path, command_path
    );
    let mut args: Vec<String> = vec!["-s".to_string(), device_id.clone(), "shell".to_string()];
    let ls_mode = android_ls_mode(&package);
    let ls_cmd = ["ls", ls_mode, &command_path];
    args.extend(shell_args(&package, &ls_cmd));
    let out = std::process::Command::new(adb)
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(err) = not_debuggable_error(&stderr) {
        return Err(err);
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let preview: String = text.lines().take(8).collect::<Vec<_>>().join(" | ");
    eprintln!(
        "[android:list] status={} stdout_lines={} stdout_preview={} stderr={}",
        out.status,
        text.lines().count(),
        preview,
        stderr.trim()
    );
    Ok(parse_adb_ls(&text, &display_path))
}

pub fn collect_android_download_files(
    device_id: &str,
    remote_path: &str,
    local_path: &str,
    package: Option<String>,
    is_dir: bool,
) -> Result<Vec<DownloadFile>, String> {
    if !is_dir {
        return Ok(vec![DownloadFile {
            remote_path: remote_path.to_string(),
            local_path: local_path.to_string(),
        }]);
    }

    let entries = list_android_files(device_id.to_string(), remote_path.to_string(), package.clone())?;
    let mut files = Vec::new();
    for entry in entries {
        let child_local = Path::new(local_path).join(&entry.name);
        if entry.is_dir {
            files.extend(collect_android_download_files(
                device_id,
                &entry.path,
                child_local.to_string_lossy().as_ref(),
                package.clone(),
                true,
            )?);
        } else {
            files.push(DownloadFile {
                remote_path: entry.path,
                local_path: child_local.to_string_lossy().to_string(),
            });
        }
    }
    Ok(files)
}

fn android_download_file_full(device_id: &str, remote_path: &str, local_path: &str, package: Option<&str>) -> Result<(), String> {
    if let Some(parent) = Path::new(local_path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let adb = crate::bin_path::resolve("adb")?;
    match package {
        None => {
            let status = std::process::Command::new(&adb)
                .args(["-s", device_id, "pull", remote_path, local_path])
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() { Ok(()) } else { Err("adb pull failed".to_string()) }
        }
        Some(pkg) => {
            let mut args: Vec<String> = vec!["-s".to_string(), device_id.to_string(), "shell".to_string()];
            args.extend(shell_args(&Some(pkg.to_string()), &["cat", remote_path]));
            let out = std::process::Command::new(&adb)
                .args(&args)
                .output()
                .map_err(|e| e.to_string())?;
            let stderr = String::from_utf8_lossy(&out.stderr);
            if let Some(err) = not_debuggable_error(&stderr) {
                return Err(err);
            }
            fs::write(local_path, &out.stdout).map_err(|e| e.to_string())?;
            if out.status.success() { Ok(()) } else { Err("run-as cat failed".to_string()) }
        }
    }
}

fn android_app_full_path(package: &str, path: &str) -> String {
    let normalized = crate::file_ops::normalize_path(path);
    let root = pkg_root(package);
    if normalized == root || normalized.starts_with(&format!("{}/", root)) {
        normalized
    } else {
        crate::file_ops::join_path(&root, path)
    }
}

fn android_app_relative_path(package: &str, full_path: &str) -> String {
    let root = pkg_root(package);
    full_path
        .strip_prefix(&root)
        .map(|path| if path.is_empty() { "/".to_string() } else { path.to_string() })
        .unwrap_or_else(|| full_path.to_string())
}

fn android_download_recursive(device_id: &str, remote_path: &str, local_path: &str, package: &str) -> Result<(), String> {
    let entries = list_android_files(device_id.to_string(), remote_path.to_string(), Some(package.to_string()))?;
    let is_leaf = entries.len() == 1
        && android_app_relative_path(package, &entries[0].path) == crate::file_ops::normalize_path(remote_path)
        && !entries[0].is_dir;
    if is_leaf {
        let full_remote = crate::file_ops::join_path(&pkg_root(package), remote_path);
        return android_download_file_full(device_id, &full_remote, local_path, Some(package));
    }

    fs::create_dir_all(local_path).map_err(|e| e.to_string())?;
    for entry in entries {
        let child_local = Path::new(local_path).join(&entry.name);
        if entry.is_dir {
            let child_remote = android_app_relative_path(package, &entry.path);
            android_download_recursive(device_id, &child_remote, child_local.to_string_lossy().as_ref(), package)?;
        } else {
            android_download_file_full(device_id, &entry.path, child_local.to_string_lossy().as_ref(), Some(package))?;
        }
    }
    Ok(())
}

/// Download a single file or directory. External storage uses `adb pull` directly.
/// App-container paths recurse through `list_android_files` and `run-as cat`.
/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn android_download_dir(
    device_id: String,
    remote_path: String,
    local_path: String,
    package: Option<String>,
) -> Result<(), String> {
    check_device_authorized(&device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    match package.as_ref() {
        None => {
            let remote_path = crate::file_ops::normalize_path(&safe_remote);
            android_download_file_full(&device_id, &remote_path, &local_path, None)
        }
        Some(pkg) => {
            let full_remote = android_app_full_path(pkg, &safe_remote);
            let remote_path = android_app_relative_path(pkg, &full_remote);
            android_download_recursive(&device_id, &remote_path, &local_path, pkg)
        }
    }
}

pub fn android_download(
    device_id: String,
    remote_path: String,
    local_path: String,
    package: Option<String>,
) -> Result<(), String> {
    check_device_authorized(&device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    match package.as_ref() {
        None => {
            let remote_path = crate::file_ops::normalize_path(&safe_remote);
            android_download_file_full(&device_id, &remote_path, &local_path, None)
        }
        Some(pkg) => {
            let remote_path = android_app_full_path(pkg, &safe_remote);
            android_download_file_full(&device_id, &remote_path, &local_path, Some(pkg))
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
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote_path = match &package {
        Some(pkg) => crate::file_ops::join_path(&pkg_root(pkg), &safe_remote),
        None => crate::file_ops::normalize_path(&safe_remote),
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

#[tauri::command(async)]
pub fn android_delete(
    device_id: String,
    remote_path: String,
    package: Option<String>,
) -> Result<(), String> {
    check_device_authorized(&device_id)?;
    let adb = crate::bin_path::resolve("adb")?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote_path = match &package {
        Some(pkg) => crate::file_ops::join_path(&pkg_root(pkg), &safe_remote),
        None => crate::file_ops::normalize_path(&safe_remote),
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
    fn test_parse_adb_ls_treats_symlink_directory_as_directory() {
        let output = "lrwxrwxrwx 1 root root 10 2024-01-01 12:00 shared -> /storage/emulated/0/Shared\n";
        let entries = parse_adb_ls(output, "/sdcard");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "shared");
    }

    #[test]
    fn test_parse_adb_ls_skips_dot_entries() {
        let output = "drwxr-xr-x 2 root root 4096 2024-01-01 12:00 .\ndrwxr-xr-x 2 root root 4096 2024-01-01 12:00 ..\n-rw-r--r-- 1 root root 100 2024-01-01 12:00 file.txt\n";
        let entries = parse_adb_ls(output, "/sdcard");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
    }

    #[test]
    fn test_resolve_external_storage_root_parses_symlink_target() {
        let output = "lrw-r--r-- 1 root root 21 2021-12-03 17:15 /sdcard -> /storage/self/primary\n";
        let target = output
            .lines()
            .find_map(|line| line.split_once(" -> ").map(|(_, target)| target.trim().to_string()))
            .map(|target| crate::file_ops::normalize_path(&target));
        assert_eq!(target.as_deref(), Some("/storage/self/primary"));
    }

    #[test]
    fn test_requires_run_as_for_app_data_paths() {
        assert!(requires_run_as("/data/data/com.example.app/files"));
        assert!(requires_run_as("/data/user/0/com.example.app/files"));
        assert!(!requires_run_as("/sdcard/DCIM"));
        assert!(!requires_run_as("/storage/emulated/0/Download"));
    }

    #[test]
    fn test_android_ls_mode_uses_follow_symlinks_for_external_storage() {
        let package = Some("com.example.app".to_string());
        let external: Option<String> = None;
        assert_eq!(android_ls_mode(&external), "-laL");
        assert_eq!(android_ls_mode(&package), "-la");
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
