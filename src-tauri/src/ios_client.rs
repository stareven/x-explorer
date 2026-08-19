use crate::types::{AppInfo, Device, DownloadFile, FileEntry, IosDeleteTarget};
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
/// device is disconnected or hasn't approved this host yet. Called once per
/// job by `TransferQueue::run_job` (was previously called per file inside
/// `ios_download` / `ios_upload` — N × 1.2s of pure idle overhead for an
/// N-file folder). Also still called directly by `list_ios_files` and
/// `ios_delete`, which run as one-shot tauri commands rather than through
/// the transfer queue.
pub(crate) fn check_ios_trusted(device_id: &str) -> Result<(), String> {
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

/// Recursively walk the iOS document subtree, returning every leaf file with
/// its absolute device-side path and its mirrored local destination path.
///
/// Optimization vs. the original (now-fixed) N+1 pattern: every directory
/// level used to issue both `afcclient info <dir>` (to determine `is_dir`) and
/// `afcclient -- ls -l <dir>` (to list entries). `ls -l`'s first column
/// already encodes `is_dir` (`drwx…` vs `-rw-…`), so the `info` call per
/// directory level is dropped. The fetcher and progress callback are passed
/// in so the recursive walk can be unit-tested with fake listings and so the
/// caller (prepare phase) can surface a growing "N files found" count to the
/// UI during enumeration.
fn collect_ios_download_files_recursive(
    remote: &str,
    user_remote: &str,
    local: &str,
    out: &mut Vec<DownloadFile>,
    fetch_listing: &mut dyn FnMut(&str) -> Result<Vec<(String, AfcFileInfo)>, String>,
    on_progress: &mut dyn FnMut(usize),
) -> Result<(), String> {
    let entries = fetch_listing(remote)?;
    for (name, info) in entries {
        let child_remote = crate::file_ops::join_path(remote, &name);
        let child_user_remote = crate::file_ops::join_path(user_remote, &name);
        let child_local = Path::new(local).join(&name);
        if info.is_dir {
            collect_ios_download_files_recursive(
                &child_remote,
                &child_user_remote,
                child_local.to_string_lossy().as_ref(),
                out,
                fetch_listing,
                on_progress,
            )?;
        } else {
            out.push(DownloadFile {
                remote_path: child_user_remote,
                local_path: child_local.to_string_lossy().into_owned(),
            });
        }
    }
    on_progress(out.len());
    Ok(())
}

/// Recursively walk an iOS document subtree and return every entry that needs
/// to be removed, in **topological order** (deepest entries first, the parent
/// directory's `rmdir` queued after all of its own descendants).
///
/// The output drives two distinct execution waves in `run_job`:
/// - **main (parallel)**: every leaf file. These are independent and run
///   concurrently via `run_ops_parallel` (up to `MAX_JOB_PARALLELISM=3`).
/// - **follow-up (serial)**: every directory's `rmdir`, executed in the same
///   order this function produces — deepest first — so a parent `rmdir` only
///   fires after every child subdirectory has already been removed. This
///   eliminates the "directory not empty" race that flat parallel execution
///   would otherwise produce when a parent and its subdirectory both end up
///   in flight at the same time.
///
/// Reuses the same `fetch_listing` / `on_progress` shape as
/// `collect_ios_download_files_recursive` for testability and uniform progress
/// reporting through `prepare_ops`.
fn collect_ios_delete_targets_recursive(
    remote: &str,
    user_remote: &str,
    out: &mut Vec<IosDeleteTarget>,
    fetch_listing: &mut dyn FnMut(&str) -> Result<Vec<(String, AfcFileInfo)>, String>,
    on_progress: &mut dyn FnMut(usize),
) -> Result<(), String> {
    let make_user_child = |parent: &str, child_name: &str| -> String {
        if parent.is_empty() {
            child_name.to_string()
        } else {
            format!("{}/{}", parent, child_name)
        }
    };
    let entries = fetch_listing(remote)?;
    // Two passes: descend into every subdirectory first (so all of their
    // targets are queued before anything at this level), then push file
    // targets. This guarantees a strict deepest-first ordering across the
    // whole subtree — a sibling file at the current level never appears
    // ahead of a file that lives in a sibling subdirectory.
    for (name, info) in entries.iter().filter(|(_, info)| info.is_dir) {
        let child_remote = crate::file_ops::join_path(remote, name);
        let child_user_remote = make_user_child(user_remote, name);
        collect_ios_delete_targets_recursive(
            &child_remote,
            &child_user_remote,
            out,
            fetch_listing,
            on_progress,
        )?;
    }
    for (name, info) in entries.iter().filter(|(_, info)| !info.is_dir) {
        let child_user_remote = make_user_child(user_remote, name);
        out.push(IosDeleteTarget {
            remote_path: child_user_remote,
            is_dir: false,
        });
    }
    // Queue the directory's own removal *after* every entry inside it has
    // been pushed. For the top-level call this is the root directory the
    // caller asked us to delete; for recursive calls it's each subdirectory.
    out.push(IosDeleteTarget {
        remote_path: user_remote.to_string(),
        is_dir: true,
    });
    on_progress(out.len());
    Ok(())
}

/// Walk the subtree rooted at `remote_path` and return every entry that needs
/// to be removed, in topological order. Trust check is hoisted to
/// `TransferQueue::run_job` (was previously redundant here — see commit
/// "ios_delete: drop redundant check_ios_trusted"). Single top-level `info`
/// call to determine file-vs-directory for the user-supplied root; once we're
/// inside the recursion, `ls -l`'s first column (`drwx…` vs `-rw-…`) already
/// encodes `is_dir`, so per-level `info` calls are dropped (same fix that
/// `collect_ios_download_files` got).
pub fn collect_ios_delete_targets(
    device_id: &str,
    bundle_id: &str,
    remote_path: &str,
    mut on_progress: impl FnMut(usize),
) -> Result<Vec<IosDeleteTarget>, String> {
    let safe_remote = crate::file_ops::sanitize_relative_path(remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    let user_remote = crate::file_ops::normalize_path(&safe_remote);

    let info_out = run_afcclient(device_id, bundle_id, &["info", &remote])?;
    let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
    let info = parse_afcclient_info(&info_text)
        .ok_or_else(|| "无法识别 iOS 文件信息".to_string())?;

    let mut targets = Vec::new();
    if info.is_dir {
        let mut fetch_listing = |remote_dir: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            let ls_out = run_afcclient(device_id, bundle_id, &["--", "ls", "-l", remote_dir])?;
            let ls_text = String::from_utf8_lossy(&ls_out.stdout).to_string();
            if !ls_out.status.success() || ls_text.contains("Error:") {
                return Err(afc_error_message(ls_text.trim(), bundle_id));
            }
            Ok(parse_afcclient_ls_long(&ls_text))
        };
        collect_ios_delete_targets_recursive(
            &remote,
            &user_remote,
            &mut targets,
            &mut fetch_listing,
            &mut on_progress,
        )?;
        on_progress(targets.len());
    } else {
        targets.push(IosDeleteTarget {
            remote_path: user_remote,
            is_dir: false,
        });
        on_progress(targets.len());
    }
    Ok(targets)
}

pub fn collect_ios_download_files(
    device_id: &str,
    bundle_id: &str,
    remote_path: &str,
    local_path: &str,
    mut on_progress: impl FnMut(usize),
) -> Result<Vec<DownloadFile>, String> {
    // Trust check is hoisted to TransferQueue::run_job; this function no longer
    // re-runs it. The previous behavior called `ideviceinfo` here AND again
    // per file inside the worker, which on a 50-file directory added ~60s of
    // pure idle subprocess startup before any download began.
    let safe_remote = crate::file_ops::sanitize_relative_path(remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    let user_remote = crate::file_ops::normalize_path(&safe_remote);

    // Single top-level info call: we still need to know if the user-supplied
    // path is a file or a directory before deciding whether to recurse. Once
    // we're inside the recursion, `ls -l`'s first column (`drwx…` vs `-rw-…`)
    // already encodes `is_dir`, so we drop the per-level `info` calls that
    // the previous N+1 pattern issued.
    let info_out = run_afcclient(device_id, bundle_id, &["info", &remote])?;
    let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
    let info = parse_afcclient_info(&info_text)
        .ok_or_else(|| "无法识别 iOS 文件信息".to_string())?;

    let mut files = Vec::new();
    if info.is_dir {
        let mut fetch_listing = |remote_dir: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            let ls_out = run_afcclient(device_id, bundle_id, &["--", "ls", "-l", remote_dir])?;
            let ls_text = String::from_utf8_lossy(&ls_out.stdout).to_string();
            if !ls_out.status.success() || ls_text.contains("Error:") {
                return Err(afc_error_message(ls_text.trim(), bundle_id));
            }
            Ok(parse_afcclient_ls_long(&ls_text))
        };
        collect_ios_download_files_recursive(
            &remote,
            &user_remote,
            local_path,
            &mut files,
            &mut fetch_listing,
            &mut on_progress,
        )?;
        on_progress(files.len());
    } else {
        files.push(DownloadFile {
            remote_path: user_remote,
            local_path: local_path.to_string(),
        });
        on_progress(files.len());
    }
    Ok(files)
}

/// Download a single file or directory tree from an iOS app container.
/// Trust check is hoisted to `TransferQueue::run_job`; this function no longer
/// re-runs it.
pub fn ios_download_dir(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    ios_download_recursive(&device_id, &bundle_id, &remote, &local_path)
}

/// Fast path for a known-file download (the only path actually reached from
/// `run_op(JobOp::IosDownload)`, since `prepare_ops` expands every directory
/// into per-file ops before the worker touches them). Skips both the trust
/// check (hoisted) and the per-file `info` probe (the file vs dir
/// distinction is already resolved by `collect_ios_download_files`).
pub fn ios_download(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    if let Some(parent) = Path::new(&local_path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let out = run_afcclient(&device_id, &bundle_id, &["get", &remote, &local_path])?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(afc_error_message(&stderr, &bundle_id))
    }
}

/// Not a #[tauri::command] — called internally by transfer_queue only.
/// Trust check is hoisted to `TransferQueue::run_job`.
pub fn ios_upload(device_id: String, bundle_id: String, local_path: String, remote_path: String) -> Result<(), String> {
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

    /// Helper: build a non-directory AfcFileInfo with the given size.
    fn file_info(size: u64) -> AfcFileInfo {
        AfcFileInfo { is_dir: false, size, modified: None }
    }

    /// Helper: build a directory AfcFileInfo.
    fn dir_info() -> AfcFileInfo {
        AfcFileInfo { is_dir: true, size: 0, modified: None }
    }

    /// Counts of progress callback invocations are captured here. Uses
    /// `Rc<RefCell<_>>` so both the returned callback and the returned counts
    /// handle point at the same underlying Vec — `RefCell::clone()` is a deep
    /// copy and would not share the buffer.
    fn make_progress_recorder() -> (
        impl FnMut(usize),
        std::rc::Rc<std::cell::RefCell<Vec<usize>>>,
    ) {
        let counts: std::rc::Rc<std::cell::RefCell<Vec<usize>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let counts_cell = counts.clone();
        let cb = move |n: usize| {
            counts_cell.borrow_mut().push(n);
        };
        (cb, counts)
    }

    /// New behavior: the recursive walk uses `is_dir` from `ls -l`'s metadata
    /// directly and accepts a listing fetcher + progress callback for testability.
    #[test]
    fn test_collect_ios_download_files_recursive_collects_files_from_flat_dir() {
        let mut fetch = |_remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            Ok(vec![
                ("a.txt".to_string(), file_info(1)),
                ("b.txt".to_string(), file_info(2)),
            ])
        };
        let (mut progress, _counts) = make_progress_recorder();
        let mut out: Vec<DownloadFile> = Vec::new();
        collect_ios_download_files_recursive(
            "/root",
            "/root",
            "/local",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].remote_path, "/root/a.txt");
        assert_eq!(out[0].local_path, "/local/a.txt");
        assert_eq!(out[1].remote_path, "/root/b.txt");
        assert_eq!(out[1].local_path, "/local/b.txt");
    }

    /// Subdirectories are recursed; their leaves are pushed with nested
    /// relative paths that mirror the directory tree shape.
    #[test]
    fn test_collect_ios_download_files_recursive_recurses_into_subdirs() {
        let mut fetch = |remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            match remote {
                "/root" => Ok(vec![
                    ("a.txt".to_string(), file_info(1)),
                    ("sub".to_string(), dir_info()),
                ]),
                "/root/sub" => Ok(vec![
                    ("b.txt".to_string(), file_info(2)),
                ]),
                _ => Err(format!("unexpected fetch: {remote}")),
            }
        };
        let (mut progress, _counts) = make_progress_recorder();
        let mut out: Vec<DownloadFile> = Vec::new();
        collect_ios_download_files_recursive(
            "/root",
            "/root",
            "/local",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].remote_path, "/root/a.txt");
        assert_eq!(out[1].remote_path, "/root/sub/b.txt");
        assert_eq!(out[1].local_path, "/local/sub/b.txt");
    }

    /// Each directory level reports its cumulative file count to the progress
    /// callback, so the caller can surface "preparing... N found" to the UI.
    /// For a leaf dir the count after processing equals the cumulative total;
    /// nested dirs emit once per level (leaf first, then parents as recursion
    /// unwinds), which lets the UI show the count growing as the walker goes
    /// deeper.
    #[test]
    fn test_collect_ios_download_files_recursive_emits_progress_per_level() {
        let mut fetch = |remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            match remote {
                "/root" => Ok(vec![
                    ("a.txt".to_string(), file_info(1)),
                    ("sub".to_string(), dir_info()),
                ]),
                "/root/sub" => Ok(vec![
                    ("b.txt".to_string(), file_info(2)),
                ]),
                _ => Err(format!("unexpected fetch: {remote}")),
            }
        };
        let (mut progress, counts) = make_progress_recorder();
        let mut out: Vec<DownloadFile> = Vec::new();
        collect_ios_download_files_recursive(
            "/root",
            "/root",
            "/local",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap();
        // /root/sub emits after its single leaf (count = 2); /root emits again
        // after the recursion returns (count still 2). Both reports show the
        // cumulative file count discovered so far.
        assert_eq!(*counts.borrow(), vec![2, 2]);
        assert_eq!(out.len(), 2);
    }

    /// Errors from the listing fetcher propagate immediately rather than being
    /// silently swallowed.
    #[test]
    fn test_collect_ios_download_files_recursive_propagates_fetch_errors() {
        let mut fetch = |remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            if remote == "/root" {
                Err("Permission denied (10)".to_string())
            } else {
                Ok(vec![])
            }
        };
        let (mut progress, _counts) = make_progress_recorder();
        let mut out: Vec<DownloadFile> = Vec::new();
        let err = collect_ios_download_files_recursive(
            "/root",
            "/root",
            "/local",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap_err();
        assert!(err.contains("Permission denied"), "expected permission error, got: {err}");
        assert!(out.is_empty());
    }

    /// After the hoist, `ios_download` does not run `check_ios_trusted`. With
    /// a traversal path on a non-existent device, the error must come from
    /// path sanitization, never from a trust/untrust check.
    #[test]
    fn test_ios_download_sanitizes_remote_path_before_subprocess() {
        let result = ios_download(
            "nonexistent-device-zzz".to_string(),
            "com.example.bundle".to_string(),
            "../etc/passwd".to_string(),
            "/tmp/x".to_string(),
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("上级目录") || err.contains("路径"),
            "expected sanitize error, got: {err}"
        );
        assert!(
            !err.contains("未连接") && !err.contains("信任"),
            "trust check should not run, got: {err}"
        );
    }

    /// Mirror of the above for `ios_upload`: the trust check used to run
    /// before path sanitization; it must no longer run inside this function.
    #[test]
    fn test_ios_upload_sanitizes_remote_path_before_subprocess() {
        let result = ios_upload(
            "nonexistent-device-zzz".to_string(),
            "com.example.bundle".to_string(),
            "/tmp/local-file".to_string(),
            "../etc/passwd".to_string(),
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("上级目录") || err.contains("路径"),
            "expected sanitize error, got: {err}"
        );
        assert!(
            !err.contains("未连接") && !err.contains("信任"),
            "trust check should not run, got: {err}"
        );
    }

    #[test]
    fn test_collect_ios_delete_targets_recursive_collects_files_from_flat_dir() {
        let mut fetch = |_remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            Ok(vec![
                ("a.txt".to_string(), file_info(1)),
                ("b.txt".to_string(), file_info(2)),
            ])
        };
        let (mut progress, _counts) = make_progress_recorder();
        let mut out: Vec<crate::types::IosDeleteTarget> = Vec::new();
        collect_ios_delete_targets_recursive(
            "/Documents/root",
            "root",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap();
        // Two leaf files first (topological: deepest first), then the now-empty
        // root directory itself last so its `rmdir` happens after both files.
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].remote_path, "root/a.txt");
        assert!(!out[0].is_dir);
        assert_eq!(out[1].remote_path, "root/b.txt");
        assert!(!out[1].is_dir);
        assert_eq!(out[2].remote_path, "root");
        assert!(out[2].is_dir);
    }

    #[test]
    fn test_collect_ios_delete_targets_recursive_emits_dir_target_after_children() {
        let mut fetch = |remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            match remote {
                "/Documents/root" => Ok(vec![
                    ("a.txt".to_string(), file_info(1)),
                    ("sub".to_string(), dir_info()),
                ]),
                "/Documents/root/sub" => Ok(vec![
                    ("b.txt".to_string(), file_info(2)),
                ]),
                _ => Err(format!("unexpected fetch: {remote}")),
            }
        };
        let (mut progress, _counts) = make_progress_recorder();
        let mut out: Vec<crate::types::IosDeleteTarget> = Vec::new();
        collect_ios_delete_targets_recursive(
            "/Documents/root",
            "root",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap();
        // Topological deepest-first: file under sub, then sub, then file under
        // root, then root. So the parent's `rmdir` always comes after its own
        // `rmdir`'s children have been queued.
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].remote_path, "root/sub/b.txt");
        assert!(!out[0].is_dir);
        assert_eq!(out[1].remote_path, "root/sub");
        assert!(out[1].is_dir);
        assert_eq!(out[2].remote_path, "root/a.txt");
        assert!(!out[2].is_dir);
        assert_eq!(out[3].remote_path, "root");
        assert!(out[3].is_dir);
    }

    #[test]
    fn test_collect_ios_delete_targets_recursive_propagates_fetch_errors() {
        let mut fetch = |_remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            Err("Permission denied (10)".to_string())
        };
        let (mut progress, _counts) = make_progress_recorder();
        let mut out: Vec<crate::types::IosDeleteTarget> = Vec::new();
        let err = collect_ios_delete_targets_recursive(
            "/Documents/root",
            "root",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap_err();
        assert!(err.contains("Permission denied"));
        assert!(out.is_empty());
    }

    #[test]
    fn test_collect_ios_delete_targets_empty_dir_returns_only_self() {
        let mut fetch = |_remote: &str| -> Result<Vec<(String, AfcFileInfo)>, String> {
            Ok(vec![])
        };
        let (mut progress, _counts) = make_progress_recorder();
        let mut out: Vec<crate::types::IosDeleteTarget> = Vec::new();
        collect_ios_delete_targets_recursive(
            "/Documents/empty",
            "empty",
            &mut out,
            &mut fetch,
            &mut progress,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].remote_path, "empty");
        assert!(out[0].is_dir);
    }
}
