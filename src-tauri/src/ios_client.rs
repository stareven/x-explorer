use crate::types::{AppInfo, Device, FileEntry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Process-wide cache of active ifuse mounts, keyed by "device_id:bundle_id".
static MOUNTS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Parse output of `idevice_id -l` into device ID list.
pub fn parse_idevice_ids(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Parse output of `ideviceinstaller -l` into AppInfo list.
/// Format: CFBundleIdentifier - CFBundleVersion - CFBundleDisplayName
pub fn parse_ideviceinstaller_list(output: &str) -> Vec<AppInfo> {
    output
        .lines()
        .filter(|line| line.contains(" - "))
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, " - ").collect();
            if parts.len() < 3 {
                return None;
            }
            Some(AppInfo {
                bundle_id: parts[0].trim().to_string(),
                name: parts[2].trim().to_string(),
            })
        })
        .collect()
}

/// Parse output of `ideviceinstaller -u <udid> list -a CFBundleIdentifier -a
/// UIFileSharingEnabled` into the set of bundle ids that have
/// UIFileSharingEnabled=true. Only these apps can be browsed via
/// `afcclient --documents <bundle_id>` (house_arrest's VendDocuments
/// requires the entitlement; VendContainer / --container doesn't work for
/// regular apps even when this flag is true, confirmed against a real
/// device — see design doc).
pub fn parse_file_sharing_enabled_ids(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1) // header: "CFBundleIdentifier, UIFileSharingEnabled"
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(2, ',').collect();
            if parts.len() != 2 {
                return None;
            }
            let bundle_id = parts[0].trim();
            let flag = parts[1].trim();
            if flag == "true" && !bundle_id.is_empty() {
                Some(bundle_id.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Returns true if an ideviceinfo/idevicepair stderr message indicates the host
/// is not yet trusted by the device (user hasn't tapped "Trust This Computer").
pub fn is_untrusted_error(stderr: &str) -> bool {
    stderr.contains("Trust") || stderr.contains("PairingDialogResponsePending")
}

/// Runs `ideviceinfo -u <udid>` and returns a clear, user-facing error if the
/// device is disconnected or hasn't approved this host yet. Called before any
/// operation that would otherwise fail with a much less clear ifuse/idevice
/// error further down the call chain (mount, list, download, upload, delete).
fn check_ios_trusted(device_id: &str) -> Result<(), String> {
    let out = run_idevice("ideviceinfo", &["-u", device_id])?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if is_untrusted_error(&stderr) {
        return Err("设备待信任或未授权，请在设备上确认后重试".to_string());
    }
    if !out.status.success() {
        return Err("设备未连接".to_string());
    }
    Ok(())
}

/// Parse `afcclient ... ls <path>` stdout: one entry name per line.
pub fn parse_afcclient_ls(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Parsed subset of `afcclient ... info <path>` JSON output that FileEntry needs.
pub struct AfcFileInfo {
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
}

/// Parse `afcclient ... info <path>` stdout (JSON) into an AfcFileInfo.
/// Returns None if the output isn't valid JSON or is missing required fields.
pub fn parse_afcclient_info(output: &str) -> Option<AfcFileInfo> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    let is_dir = value.get("st_ifmt")?.as_str()? == "S_IFDIR";
    let size = value.get("st_size")?.as_u64()?;
    let modified = value
        .get("st_mtime")
        .and_then(|v| v.as_u64())
        .map(|nanos| nanos / 1_000_000_000);
    Some(AfcFileInfo { is_dir, size, modified })
}

/// List files in a mounted ifuse path (uses std::fs).
/// TODO(Task 3): remove once `list_ios_files`/`ios_download`/`ios_upload`/
/// `ios_delete` are rewritten to use `run_afcclient` instead of an ifuse mount.
fn list_mounted_dir(mount_path: &str, sub_path: &str) -> Result<Vec<FileEntry>, String> {
    let full_path = PathBuf::from(crate::file_ops::join_path(mount_path, sub_path));
    let normalized_sub_path = crate::file_ops::normalize_path(sub_path);
    let entries = std::fs::read_dir(&full_path)
        .map_err(|e| format!("Cannot read {}: {}", full_path.display(), e))?;
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        result.push(FileEntry {
            path: crate::file_ops::join_path(&normalized_sub_path, &name),
            name,
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified,
        });
    }
    Ok(result)
}

fn run_idevice(bin_name: &str, args: &[&str]) -> Result<std::process::Output, String> {
    let bin = crate::bin_path::resolve(bin_name)?;
    std::process::Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| e.to_string())
}

/// Runs `afcclient -u <device_id> --documents <bundle_id> <args...>`.
/// All iOS app file access goes through this — one-shot, stateless subprocess
/// call per operation (no persistent mount to manage). `--container` is
/// deliberately never used: confirmed against a real device that it fails
/// with `InstallationLookupFailed` for regular (non-provisioned) apps even
/// when UIFileSharingEnabled=true, while `--documents` works correctly.
fn run_afcclient(device_id: &str, bundle_id: &str, args: &[&str]) -> Result<std::process::Output, String> {
    let bin = crate::bin_path::resolve("afcclient")?;
    let mut full_args: Vec<&str> = vec!["-u", device_id, "--documents", bundle_id];
    full_args.extend_from_slice(args);
    std::process::Command::new(bin)
        .args(&full_args)
        .output()
        .map_err(|e| e.to_string())
}

/// Builds the absolute afcclient-side path for a user-facing relative path.
/// The `--documents` jail exposes `/Documents` as the only readable subtree
/// (listing the literal `/` root itself returns "Permission denied" —
/// confirmed on a real device; `info /` and `ls /Documents` both work fine).
/// `sub_path` here is already sanitized (no `..`) by the caller.
fn documents_path(sub_path: &str) -> String {
    crate::file_ops::join_path("/Documents", sub_path)
}

/// Returns a clear, user-facing error for known house_arrest/AFC failure
/// modes instead of the raw afcclient stderr.
fn afc_error_message(stderr: &str, bundle_id: &str) -> String {
    if stderr.contains("InstallationLookupFailed") {
        format!("应用 {} 未开启文件共享，无法访问其文档目录", bundle_id)
    } else if stderr.contains("Permission denied") {
        "无权限访问该路径".to_string()
    } else if stderr.trim().is_empty() {
        format!("操作失败: {}", bundle_id)
    } else {
        stderr.trim().to_string()
    }
}

#[tauri::command]
pub fn list_ios_devices() -> Result<Vec<Device>, String> {
    let out = run_idevice("idevice_id", &["-l"])?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let ids = parse_idevice_ids(&text);
    let mut devices = Vec::new();
    for id in ids {
        let info_out = run_idevice("ideviceinfo", &["-u", &id])?;
        let stderr = String::from_utf8_lossy(&info_out.stderr);
        let status = if is_untrusted_error(&stderr) {
            "unauthorized".to_string()
        } else {
            "connected".to_string()
        };
        devices.push(Device {
            name: id.clone(),
            id,
            platform: "ios".to_string(),
            status,
        });
    }
    Ok(devices)
}

#[tauri::command]
pub fn list_ios_apps(device_id: String) -> Result<Vec<AppInfo>, String> {
    let sharing_out = run_idevice(
        "ideviceinstaller",
        &["-u", &device_id, "list", "-a", "CFBundleIdentifier", "-a", "UIFileSharingEnabled"],
    )?;
    let sharing_text = String::from_utf8_lossy(&sharing_out.stdout).to_string();
    let enabled_ids: std::collections::HashSet<String> =
        parse_file_sharing_enabled_ids(&sharing_text).into_iter().collect();

    let list_out = run_idevice("ideviceinstaller", &["-u", &device_id, "-l"])?;
    let list_text = String::from_utf8_lossy(&list_out.stdout).to_string();
    let all_apps = parse_ideviceinstaller_list(&list_text);

    Ok(all_apps
        .into_iter()
        .filter(|app| enabled_ids.contains(&app.bundle_id))
        .collect())
}

#[tauri::command]
pub fn list_ios_files(device_id: String, bundle_id: String, path: String) -> Result<Vec<FileEntry>, String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let safe_path = crate::file_ops::sanitize_relative_path(&path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    list_mounted_dir(&mount_path, &safe_path)
}

/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn ios_download(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let src = PathBuf::from(crate::file_ops::join_path(&mount_path, &safe_remote));
    std::fs::copy(&src, &local_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn ios_upload(device_id: String, bundle_id: String, local_path: String, remote_path: String) -> Result<(), String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let dst = PathBuf::from(crate::file_ops::join_path(&mount_path, &safe_remote));
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&local_path, &dst).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn ios_delete(device_id: String, bundle_id: String, remote_path: String) -> Result<(), String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let target = PathBuf::from(crate::file_ops::join_path(&mount_path, &safe_remote));
    if target.is_dir() {
        std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    } else {
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Explicitly unmount a container, e.g. when the user switches to a different app
/// or the device disconnects. Safe to call even if not currently mounted.
#[tauri::command]
pub fn ios_unmount_container(device_id: String, bundle_id: String) -> Result<(), String> {
    let key = format!("{}:{}", device_id, bundle_id);
    let mut guard = MOUNTS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(mount_path) = map.remove(&key) {
        let _ = std::process::Command::new("umount").arg(&mount_path).status();
    }
    Ok(())
}

/// Mount an iOS app container via ifuse, returning the mount path.
/// Reuses an existing mount for the same device+bundle if already mounted.
/// Checks trust state first so a disconnected/untrusted device produces the
/// same clear "待信任/未授权" error used elsewhere, instead of a raw
/// "ifuse mount failed" message with no actionable cause.
fn mount_ios_container(device_id: &str, bundle_id: &str) -> Result<String, String> {
    let key = format!("{}:{}", device_id, bundle_id);
    let mut guard = MOUNTS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);

    if let Some(existing) = map.get(&key) {
        return Ok(existing.clone());
    }
    drop(guard);

    check_ios_trusted(device_id)?;

    let mut guard = MOUNTS.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);

    // Re-check: another thread may have mounted while we were checking trust.
    if let Some(existing) = map.get(&key) {
        return Ok(existing.clone());
    }

    let mount_path = std::env::temp_dir()
        .join("x-explorer")
        .join(device_id)
        .join(bundle_id);
    std::fs::create_dir_all(&mount_path).map_err(|e| e.to_string())?;
    let mount_path_str = mount_path
        .to_str()
        .ok_or_else(|| "挂载路径包含无效字符".to_string())?;
    let ifuse = crate::bin_path::resolve("ifuse")?;
    let args = ifuse_args(device_id, bundle_id, mount_path_str);
    let status = std::process::Command::new(ifuse)
        .args(&args)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&mount_path);
        return Err(format!("挂载应用容器失败: {}", bundle_id));
    }

    let path_str = mount_path.to_string_lossy().to_string();
    map.insert(key, path_str.clone());
    Ok(path_str)
}

/// Builds the ifuse argv for mounting a container: --udid, --container, and the mount path.
fn ifuse_args(device_id: &str, bundle_id: &str, mount_path: &str) -> Vec<String> {
    vec![
        "--udid".to_string(), device_id.to_string(),
        "--container".to_string(), bundle_id.to_string(),
        mount_path.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_idevice_ids() {
        let output = "abc123def456\nxyz789uvw012\n";
        let ids = parse_idevice_ids(output);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], "abc123def456");
    }

    #[test]
    fn test_parse_idevice_ids_empty() {
        let ids = parse_idevice_ids("");
        assert_eq!(ids.len(), 0);
    }

    #[test]
    fn test_parse_ideviceinstaller_list() {
        let output = "com.example.myapp - 1.0.0 - My App\ncom.foo.bar - 2.1 - Foo Bar\n";
        let apps = parse_ideviceinstaller_list(&output);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].bundle_id, "com.example.myapp");
        assert_eq!(apps[0].name, "My App");
        assert_eq!(apps[1].bundle_id, "com.foo.bar");
        assert_eq!(apps[1].name, "Foo Bar");
    }

    #[test]
    fn test_parse_ideviceinstaller_list_skips_header() {
        let output = "Total: 2 apps\ncom.example.myapp - 1.0 - My App\n";
        let apps = parse_ideviceinstaller_list(&output);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].bundle_id, "com.example.myapp");
    }

    #[test]
    fn test_is_untrusted_error_detects_trust_pending() {
        assert!(is_untrusted_error("ERROR: Could not connect to lockdownd, error code -18 (Trust)"));
        assert!(is_untrusted_error("PairingDialogResponsePending"));
        assert!(!is_untrusted_error(""));
    }

    #[test]
    fn test_parse_file_sharing_enabled_ids_filters_true_only() {
        let output = "CFBundleIdentifier, UIFileSharingEnabled\ncn.com.gf.etj, \nrn.notes.best, true\ncom.openai.chat, \ncom.apple.Pages, true\n";
        let ids = parse_file_sharing_enabled_ids(output);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"rn.notes.best".to_string()));
        assert!(ids.contains(&"com.apple.Pages".to_string()));
        assert!(!ids.contains(&"cn.com.gf.etj".to_string()));
    }

    #[test]
    fn test_parse_file_sharing_enabled_ids_skips_header() {
        let output = "CFBundleIdentifier, UIFileSharingEnabled\ncom.example.app, true\n";
        let ids = parse_file_sharing_enabled_ids(output);
        assert_eq!(ids, vec!["com.example.app".to_string()]);
    }

    #[test]
    fn test_parse_file_sharing_enabled_ids_empty_output() {
        let ids = parse_file_sharing_enabled_ids("");
        assert_eq!(ids.len(), 0);
    }

    #[test]
    fn test_ifuse_args_builds_expected_argv() {
        let args = ifuse_args("udid123", "com.example.app", "/tmp/x-explorer/udid123/com.example.app");
        assert_eq!(
            args,
            vec![
                "--udid".to_string(),
                "udid123".to_string(),
                "--container".to_string(),
                "com.example.app".to_string(),
                "/tmp/x-explorer/udid123/com.example.app".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_afcclient_ls_splits_lines() {
        let output = "8es1421dg.be\nApplication Support.zip\n";
        let names = parse_afcclient_ls(output);
        assert_eq!(names, vec!["8es1421dg.be".to_string(), "Application Support.zip".to_string()]);
    }

    #[test]
    fn test_parse_afcclient_ls_empty() {
        let names = parse_afcclient_ls("");
        assert_eq!(names.len(), 0);
    }

    #[test]
    fn test_parse_afcclient_info_json_dir() {
        let json = "{\n  \"st_size\": 224,\n  \"st_blocks\": 0,\n  \"st_nlink\": 6,\n  \"st_ifmt\": \"S_IFDIR\",\n  \"st_mtime\": 1765271745750627872,\n  \"st_birthtime\": 1765271744308826162\n}\n";
        let info = parse_afcclient_info(json).expect("should parse");
        assert!(info.is_dir);
        assert_eq!(info.size, 224);
        assert_eq!(info.modified, Some(1765271745));
    }

    #[test]
    fn test_parse_afcclient_info_json_file() {
        let json = "{\n  \"st_size\": 42,\n  \"st_ifmt\": \"S_IFREG\",\n  \"st_mtime\": 1765271745750627872\n}\n";
        let info = parse_afcclient_info(json).expect("should parse");
        assert!(!info.is_dir);
        assert_eq!(info.size, 42);
    }

    #[test]
    fn test_parse_afcclient_info_invalid_json_returns_none() {
        assert!(parse_afcclient_info("not json").is_none());
    }

    #[test]
    fn test_documents_path_prefixes_with_documents_root() {
        assert_eq!(documents_path("photo.jpg"), "/Documents/photo.jpg");
        assert_eq!(documents_path(""), "/Documents");
        assert_eq!(documents_path("sub/dir"), "/Documents/sub/dir");
    }

    #[test]
    fn test_afc_error_message_installation_lookup_failed() {
        let msg = afc_error_message("ERROR: InstallationLookupFailed\nThe App ...", "com.example.app");
        assert_eq!(msg, "应用 com.example.app 未开启文件共享，无法访问其文档目录");
    }

    #[test]
    fn test_afc_error_message_permission_denied() {
        let msg = afc_error_message("Error: Failed to list '/': Permission denied (10)", "com.example.app");
        assert_eq!(msg, "无权限访问该路径");
    }

    #[test]
    fn test_afc_error_message_falls_back_to_raw_stderr() {
        let msg = afc_error_message("some other error", "com.example.app");
        assert_eq!(msg, "some other error");
    }
}
