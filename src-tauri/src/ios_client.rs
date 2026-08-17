use crate::types::{AppInfo, Device, DownloadFile, FileEntry};
use std::fs;
use std::path::Path;

/// Parse output of `idevice_id -l` into device ID list.
pub fn parse_idevice_ids(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn parse_ideviceinfo_device_name(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() == "DeviceName" {
            let name = value.trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        } else {
            None
        }
    })
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

/// Parse `afcclient ... ls -l <path>` stdout into (name, info) pairs, so one
/// subprocess call yields every entry's type/size/mtime directly. Format per
/// line (ls-style, name may contain spaces):
///   drwxr-xr-x    2 mobile mobile         64 10 Dec 2024 23:42:40 .Trash
/// Lines whose metadata can't be parsed degrade to a placeholder
/// (`is_dir: false, size: 0, modified: None`) so the frontend's info-probe
/// fallback (`enqueue_ios_file_info`) re-fetches just those entries.
fn strip_ansi_escape_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn parse_afcclient_ls_long(output: &str) -> Vec<(String, AfcFileInfo)> {
    let placeholder = || AfcFileInfo { is_dir: false, size: 0, modified: None };
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let line = strip_ansi_escape_codes(line);
            let parts: Vec<&str> = line.split_whitespace().collect();
            // 9 metadata fields (perms nlink owner group size day mon year time)
            // followed by the file name; rejoin remainder to preserve spaces.
            if parts.len() < 10 {
                return (line.trim().to_string(), placeholder());
            }
            let name = parts[9..].join(" ");
            let size = match parts[4].parse::<u64>() {
                Ok(s) => s,
                Err(_) => return (name, placeholder()),
            };
            let modified = parse_ls_time(parts[5], parts[6], parts[7], parts[8]);
            let is_dir = parts[0].starts_with('d');
            (name, AfcFileInfo { is_dir, size, modified })
        })
        .collect()
}

/// "10", "Dec", "2024", "23:42:40" -> naive unix seconds (UTC-assumed; afcclient
/// prints device-local time without a zone, and mtime is only used for display
/// ordering, so a few hours of skew is acceptable). Returns None if malformed.
fn parse_ls_time(day: &str, month: &str, year: &str, hms: &str) -> Option<u64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_idx = MONTHS.iter().position(|m| *m == month)?;
    let day: u64 = day.parse().ok()?;
    let year: i64 = year.parse().ok()?;
    let mut hms_parts = hms.split(':');
    let hour: u64 = hms_parts.next()?.parse().ok()?;
    let min: u64 = hms_parts.next()?.parse().ok()?;
    let sec: u64 = hms_parts.next()?.parse().ok()?;
    // Days since epoch via Howard Hinnant's civil-date algorithm.
    let y = if month_idx < 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = (month_idx as u64 + 10) % 12; // Mar=0..Feb=11
    let doy = (153 * mp + 2) / 5 + day - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // days since epoch of era start
    let days = era as u64 * 146097 + doe - 719468; // shift to unix epoch (1970-01-01)
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

fn run_idevice(bin_name: &str, args: &[&str]) -> Result<std::process::Output, String> {
    let bin = crate::bin_path::resolve(bin_name)?;
    std::process::Command::new(bin)
        .args(args)
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .env_remove("CLICOLOR_FORCE")
        .env("TERM", "dumb")
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
        let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&info_out.stderr);
        let status = if is_untrusted_error(&stderr) {
            "unauthorized".to_string()
        } else {
            "connected".to_string()
        };
        devices.push(Device {
            name: parse_ideviceinfo_device_name(&info_text).unwrap_or_else(|| id.clone()),
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

/// Lists a directory with full metadata via a single `afcclient -- ls -l`
/// subprocess call (~1.2s), which returns type/size/mtime for every entry at
/// once — replacing the previous N+1 pattern (plain `ls` + one `info` call
/// per entry at ~1.2s each), where directory icons only appeared after
/// N/8 × 1.2s of background probing and concurrent probes occasionally lost
/// the USB/lockdownd race, leaving folders misclassified as files.
/// Entries whose `ls -l` line can't be parsed come back with placeholder
/// metadata (`is_dir: false`, `size: 0`, `modified: None`); the frontend
/// re-probes just those via `enqueue_ios_file_info`.
#[tauri::command(async)]
pub fn list_ios_files(device_id: String, bundle_id: String, path: String) -> Result<Vec<FileEntry>, String> {
    check_ios_trusted(&device_id)?;
    let safe_path = crate::file_ops::sanitize_relative_path(&path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote_dir = documents_path(&safe_path);

    let out = run_afcclient(&device_id, &bundle_id, &["--", "ls", "-l", &remote_dir])?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(afc_error_message(&stderr, &bundle_id));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();

    Ok(parse_afcclient_ls_long(&text)
        .into_iter()
        .map(|(name, info)| {
            let entry_ui_path = crate::file_ops::join_path(&safe_path, &name);
            FileEntry {
                path: entry_ui_path,
                name,
                is_dir: info.is_dir,
                size: info.size,
                modified: info.modified,
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

/// Fallback metadata probe: re-fetches individual paths' metadata (type/size/
/// mtime) via `afcclient info`, bounded to `FILE_INFO_MAX_CONCURRENCY`
/// concurrent subprocesses. The primary path is `ls -l` in `list_ios_files`,
/// which returns metadata for all entries in one call; only entries whose
/// `ls -l` line failed to parse (placeholder `modified: None`) get enqueued
/// here. Returns immediately; each entry's result is pushed to the frontend
/// individually via an `ios-file-info-ready` event as soon as it's ready.
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

fn ios_download_recursive(device_id: &str, bundle_id: &str, remote: &str, local: &str) -> Result<(), String> {
    let info_out = run_afcclient(device_id, bundle_id, &["info", remote])?;
    let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
    if let Some(info) = parse_afcclient_info(&info_text) {
        if info.is_dir {
            fs::create_dir_all(local).map_err(|e| e.to_string())?;
            let ls_out = run_afcclient(device_id, bundle_id, &["--", "ls", "-l", remote])?;
            let ls_text = String::from_utf8_lossy(&ls_out.stdout).to_string();
            if !ls_out.status.success() || ls_text.contains("Error:") {
                return Err(afc_error_message(ls_text.trim(), bundle_id));
            }
            for (name, _) in parse_afcclient_ls_long(&ls_text) {
                let child_remote = crate::file_ops::join_path(remote, &name);
                let child_local = Path::new(local).join(&name);
                ios_download_recursive(device_id, bundle_id, &child_remote, child_local.to_string_lossy().as_ref())?;
            }
            Ok(())
        } else {
            if let Some(parent) = Path::new(local).parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let out = run_afcclient(device_id, bundle_id, &["get", remote, local])?;
            if out.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(afc_error_message(&stderr, bundle_id))
            }
        }
    } else {
        Err("无法识别 iOS 文件信息".to_string())
    }
}

fn collect_ios_download_files_recursive(
    device_id: &str,
    bundle_id: &str,
    remote: &str,
    user_remote: &str,
    local: &str,
    out: &mut Vec<DownloadFile>,
) -> Result<(), String> {
    let info_out = run_afcclient(device_id, bundle_id, &["info", remote])?;
    let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
    let info = parse_afcclient_info(&info_text).ok_or_else(|| "无法识别 iOS 文件信息".to_string())?;
    if info.is_dir {
        let ls_out = run_afcclient(device_id, bundle_id, &["--", "ls", "-l", remote])?;
        let ls_text = String::from_utf8_lossy(&ls_out.stdout).to_string();
        if !ls_out.status.success() || ls_text.contains("Error:") {
            return Err(afc_error_message(ls_text.trim(), bundle_id));
        }
        for (name, _) in parse_afcclient_ls_long(&ls_text) {
            let child_remote = crate::file_ops::join_path(remote, &name);
            let child_user_remote = crate::file_ops::join_path(user_remote, &name);
            let child_local = Path::new(local).join(&name);
            collect_ios_download_files_recursive(
                device_id,
                bundle_id,
                &child_remote,
                &child_user_remote,
                child_local.to_string_lossy().as_ref(),
                out,
            )?;
        }
    } else {
        out.push(DownloadFile {
            remote_path: user_remote.to_string(),
            local_path: local.to_string(),
        });
    }
    Ok(())
}

pub fn collect_ios_download_files(
    device_id: &str,
    bundle_id: &str,
    remote_path: &str,
    local_path: &str,
) -> Result<Vec<DownloadFile>, String> {
    check_ios_trusted(device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    let user_remote = crate::file_ops::normalize_path(&safe_remote);
    let mut files = Vec::new();
    collect_ios_download_files_recursive(device_id, bundle_id, &remote, &user_remote, local_path, &mut files)?;
    Ok(files)
}

/// Download a single file or directory tree from an iOS app container.
pub fn ios_download_dir(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    check_ios_trusted(&device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    ios_download_recursive(&device_id, &bundle_id, &remote, &local_path)
}

pub fn ios_download(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    ios_download_dir(device_id, bundle_id, remote_path, local_path)
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
    fn test_parse_ideviceinfo_device_name() {
        let output = "DeviceName: Alice's iPhone\nProductType: iPhone15,2\n";
        assert_eq!(parse_ideviceinfo_device_name(output), Some("Alice's iPhone".to_string()));
    }

    #[test]
    fn test_parse_ideviceinfo_device_name_missing_returns_none() {
        assert_eq!(parse_ideviceinfo_device_name("ProductType: iPhone15,2\n"), None);
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
    fn test_parse_afcclient_ls_long_dir_and_file_with_spaces() {
        let output = "-rw-r--r--    1 mobile mobile    3067147 10 Nov 2024 19:18:47 Blank 2.pages\ndrwxr-xr-x    2 mobile mobile         64 10 Dec 2024 23:42:40 .Trash\n";
        let entries = parse_afcclient_ls_long(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "Blank 2.pages");
        assert!(!entries[0].1.is_dir);
        assert_eq!(entries[0].1.size, 3067147);
        assert_eq!(entries[0].1.modified, Some(1731266327));
        assert_eq!(entries[1].0, ".Trash");
        assert!(entries[1].1.is_dir);
        assert_eq!(entries[1].1.size, 64);
    }

    #[test]
    fn test_parse_afcclient_ls_long_falls_back_to_placeholder_on_bad_size() {
        let output = "drwxr-xr-x    2 mobile mobile    notanumber 10 Dec 2024 23:42:40 dir\n";
        let entries = parse_afcclient_ls_long(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "dir");
        // Unparseable metadata degrades to placeholder so the frontend's
        // info-probe fallback re-fetches it.
        assert!(!entries[0].1.is_dir);
        assert_eq!(entries[0].1.size, 0);
        assert_eq!(entries[0].1.modified, None);
    }

    #[test]
    fn test_parse_afcclient_ls_long_empty() {
        assert_eq!(parse_afcclient_ls_long("").len(), 0);
    }

    #[test]
    fn test_parse_afcclient_ls_long_strips_ansi_colors() {
        let output = "drwxr-xr-x    2 mobile mobile         64 17 Aug 2026 10:36:12 \u{1b}[0;36mmaime\u{1b}[m\n";
        let entries = parse_afcclient_ls_long(output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "maime");
        assert!(entries[0].1.is_dir);
    }

    #[test]
    fn test_parse_afcclient_ls_long_symlink_is_not_dir() {
        let output = "lrwxr-xr-x    1 mobile mobile        11 01 Jan 2025 00:00:00 link\n";
        let entries = parse_afcclient_ls_long(output);
        assert!(!entries[0].1.is_dir);
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
