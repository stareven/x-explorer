use crate::types::{AppInfo, Device, FileEntry};

/// Parse output of `idevice_id -l` into device ID list.
pub fn parse_idevice_ids(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Parse output of `ideviceinstaller list -a CFBundleIdentifier -a
/// CFBundleDisplayName` into AppInfo list.
/// Format (CSV-like, header row + `id, "name"` per line):
///   CFBundleIdentifier, CFBundleDisplayName
///   com.apple.Pages, "Pages"
pub fn parse_ideviceinstaller_list(output: &str) -> Vec<AppInfo> {
    output
        .lines()
        .skip(1) // header: "CFBundleIdentifier, CFBundleDisplayName"
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(2, ',').collect();
            if parts.len() != 2 {
                return None;
            }
            let bundle_id = parts[0].trim();
            let name = parts[1].trim().trim_matches('"');
            if bundle_id.is_empty() {
                return None;
            }
            Some(AppInfo {
                bundle_id: bundle_id.to_string(),
                name: name.to_string(),
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

#[tauri::command(async)]
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

#[tauri::command(async)]
pub fn list_ios_apps(device_id: String) -> Result<Vec<AppInfo>, String> {
    let sharing_out = run_idevice(
        "ideviceinstaller",
        &["-u", &device_id, "list", "-a", "CFBundleIdentifier", "-a", "UIFileSharingEnabled"],
    )?;
    let sharing_text = String::from_utf8_lossy(&sharing_out.stdout).to_string();
    let enabled_ids: std::collections::HashSet<String> =
        parse_file_sharing_enabled_ids(&sharing_text).into_iter().collect();

    let list_out = run_idevice(
        "ideviceinstaller",
        &["-u", &device_id, "list", "-a", "CFBundleIdentifier", "-a", "CFBundleDisplayName"],
    )?;
    let list_text = String::from_utf8_lossy(&list_out.stdout).to_string();
    let all_apps = parse_ideviceinstaller_list(&list_text);

    Ok(all_apps
        .into_iter()
        .filter(|app| enabled_ids.contains(&app.bundle_id))
        .collect())
}

/// Lists file names in a directory without probing each entry's type/size —
/// that would mean one `afcclient info` subprocess per entry (~1.2s each due
/// to afcclient's per-invocation startup cost), which made large directories
/// take minutes to open. Instead, entries come back with placeholder metadata
/// (`is_dir: false`, `size: 0`, `modified: None`) so the list renders
/// instantly; the frontend calls `enqueue_ios_file_info` right after to fill
/// in real metadata asynchronously (see that function's doc comment).
#[tauri::command(async)]
pub fn list_ios_files(device_id: String, bundle_id: String, path: String) -> Result<Vec<FileEntry>, String> {
    check_ios_trusted(&device_id)?;
    let safe_path = crate::file_ops::sanitize_relative_path(&path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote_dir = documents_path(&safe_path);

    let out = run_afcclient(&device_id, &bundle_id, &["ls", &remote_dir])?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(afc_error_message(&stderr, &bundle_id));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let names = parse_afcclient_ls(&text);

    Ok(names
        .into_iter()
        .map(|name| {
            let entry_ui_path = crate::file_ops::join_path(&safe_path, &name);
            FileEntry {
                path: entry_ui_path,
                name,
                is_dir: false,
                size: 0,
                modified: None,
            }
        })
        .collect())
}

/// Max concurrent `afcclient info` subprocesses spawned by
/// `enqueue_ios_file_info`. Bounded to avoid overwhelming the single
/// USB/lockdownd connection with too many simultaneous processes; 8 is a
/// conservative starting point given each call's ~1.2s startup overhead is
/// dominated by process spawn, not device I/O.
const FILE_INFO_MAX_CONCURRENCY: usize = 8;

/// Kicks off a background probe of each path's real metadata (type/size/
/// mtime) via `afcclient info`, bounded to `FILE_INFO_MAX_CONCURRENCY`
/// concurrent subprocesses. Returns immediately; each entry's result is
/// pushed to the frontend individually via an `ios-file-info-ready` event
/// as soon as it's ready, rather than waiting for the whole batch — so the
/// UI can patch in real icons/sizes progressively instead of blocking on
/// the slowest entry.
#[tauri::command]
pub fn enqueue_ios_file_info(
    app: tauri::AppHandle,
    device_id: String,
    bundle_id: String,
    paths: Vec<String>,
) {
    use tauri::Emitter;
    std::thread::spawn(move || {
        let semaphore = std::sync::Arc::new((std::sync::Mutex::new(0usize), std::sync::Condvar::new()));
        std::thread::scope(|scope| {
            for path in paths {
                let device_id = &device_id;
                let bundle_id = &bundle_id;
                let app = &app;
                let semaphore = semaphore.clone();
                // Acquire a slot: block until fewer than FILE_INFO_MAX_CONCURRENCY
                // probes are in flight.
                {
                    let (lock, cvar) = &*semaphore;
                    let mut in_flight = lock.lock().unwrap();
                    while *in_flight >= FILE_INFO_MAX_CONCURRENCY {
                        in_flight = cvar.wait(in_flight).unwrap();
                    }
                    *in_flight += 1;
                }
                scope.spawn(move || {
                    let safe_path = crate::file_ops::sanitize_relative_path(&path);
                    if let Some(safe_path) = safe_path {
                        let remote_path = documents_path(&safe_path);
                        if let Ok(info_out) = run_afcclient(device_id, bundle_id, &["info", &remote_path]) {
                            let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
                            if let Some(info) = parse_afcclient_info(&info_text) {
                                let _ = app.emit(
                                    "ios-file-info-ready",
                                    crate::types::IosFileInfoReady {
                                        path,
                                        is_dir: info.is_dir,
                                        size: info.size,
                                        modified: info.modified,
                                    },
                                );
                            }
                        }
                    }
                    // Release the slot and wake one waiter, if any.
                    let (lock, cvar) = &*semaphore;
                    *lock.lock().unwrap() -= 1;
                    cvar.notify_one();
                });
            }
        });
    });
}

/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn ios_download(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    check_ios_trusted(&device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    let out = run_afcclient(&device_id, &bundle_id, &["get", &remote, &local_path])?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(afc_error_message(&stderr, &bundle_id))
    }
}

/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn ios_upload(device_id: String, bundle_id: String, local_path: String, remote_path: String) -> Result<(), String> {
    check_ios_trusted(&device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    let out = run_afcclient(&device_id, &bundle_id, &["put", &local_path, &remote])?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(afc_error_message(&stderr, &bundle_id))
    }
}

/// afcclient's one-shot `rm` has no recursive flag and — worse — reports
/// failures (e.g. "Directory not empty") on *stdout* with exit code 0, so
/// success must be detected by the absence of an "Error:" prefix in stdout
/// rather than via `status.success()` alone.
fn afc_remove(device_id: &str, bundle_id: &str, remote: &str) -> Result<(), String> {
    let out = run_afcclient(device_id, bundle_id, &["rm", remote])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || stdout.contains("Error:") {
        Err(afc_error_message(stdout.trim(), bundle_id))
    } else {
        Ok(())
    }
}

/// Depth-first recursive delete: `info` tells us whether `remote` is a
/// directory; if so, `ls` it, recurse into every entry, then remove the
/// now-empty directory itself. Files are removed directly.
fn afc_remove_recursive(device_id: &str, bundle_id: &str, remote: &str) -> Result<(), String> {
    let info_out = run_afcclient(device_id, bundle_id, &["info", remote])?;
    let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
    if let Some(info) = parse_afcclient_info(&info_text) {
        if info.is_dir {
            let ls_out = run_afcclient(device_id, bundle_id, &["ls", remote])?;
            let ls_text = String::from_utf8_lossy(&ls_out.stdout).to_string();
            for name in parse_afcclient_ls(&ls_text) {
                let child = crate::file_ops::join_path(remote, &name);
                afc_remove_recursive(device_id, bundle_id, &child)?;
            }
        }
    }
    afc_remove(device_id, bundle_id, remote)
}

#[tauri::command(async)]
pub fn ios_delete(device_id: String, bundle_id: String, remote_path: String) -> Result<(), String> {
    check_ios_trusted(&device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    afc_remove_recursive(&device_id, &bundle_id, &documents_path(&safe_remote))
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
        let output = "CFBundleIdentifier, CFBundleDisplayName\ncom.example.myapp, \"My App\"\ncom.foo.bar, \"Foo Bar\"\n";
        let apps = parse_ideviceinstaller_list(&output);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].bundle_id, "com.example.myapp");
        assert_eq!(apps[0].name, "My App");
        assert_eq!(apps[1].bundle_id, "com.foo.bar");
        assert_eq!(apps[1].name, "Foo Bar");
    }

    #[test]
    fn test_parse_ideviceinstaller_list_skips_header() {
        let output = "CFBundleIdentifier, CFBundleDisplayName\ncom.example.myapp, \"My App\"\n";
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
