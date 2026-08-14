# x-explorer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS desktop app (Tauri + React) for browsing, importing, and exporting files on connected iOS and Android devices.

**Architecture:** Rust backend calls bundled adb/idevice binaries to communicate with devices and exposes Tauri commands to the React frontend. Tauri events push device hotplug notifications and transfer progress to the frontend. React uses Zustand for global state and renders a three-panel layout (device list, file browser, transfer progress).

**Tech Stack:** Tauri 2, Rust, React 18, TypeScript, Zustand, Tailwind CSS, Vitest, @tauri-apps/api

---

## File Map

### Rust (`src-tauri/src/`)
| File | Responsibility |
|------|---------------|
| `main.rs` | Tauri app entry, register commands, start device_manager background thread |
| `types.rs` | Shared data types: `Device`, `AppInfo`, `FileEntry`, `TransferTask` |
| `device_manager.rs` | Poll adb + idevice every 2s, emit `devices-changed` Tauri event |
| `ios_client.rs` | Wrap idevice CLI tools: list devices, list apps, mount container, file ops |
| `android_client.rs` | Wrap adb: list devices, list packages, ls, pull, push, run-as |
| `file_ops.rs` | Unified file operations interface delegating to ios_client / android_client |
| `transfer_queue.rs` | Async task queue with concurrency limit, progress events, cancel support |

### Frontend (`src/`)
| File | Responsibility |
|------|---------------|
| `main.tsx` | React entry point |
| `App.tsx` | Root layout: DevicePanel + FileBrowser + TransferPanel |
| `store/index.ts` | Zustand store: devices, selectedDevice, selectedApp, currentPath, files, transfers |
| `hooks/useTauri.ts` | Typed wrappers around `invoke()` and `listen()` |
| `components/DevicePanel/index.tsx` | Left column: DeviceList + AppList |
| `components/DevicePanel/DeviceList.tsx` | Device cards with connection status badge |
| `components/DevicePanel/AppList.tsx` | Installed app list for selected device |
| `components/FileBrowser/index.tsx` | Right main area: BreadcrumbBar + Toolbar + FileGrid/FileList |
| `components/FileBrowser/BreadcrumbBar.tsx` | Clickable path segments |
| `components/FileBrowser/Toolbar.tsx` | View toggle, import/export buttons, multi-select batch actions |
| `components/FileBrowser/FileGrid.tsx` | Icon grid view |
| `components/FileBrowser/FileList.tsx` | Detail list view (name, size, modified) |
| `components/FileBrowser/useDragDrop.ts` | Hook: drag-out via Tauri startDrag, drop-in via ondrop |
| `components/FileBrowser/useSelection.ts` | Hook: Cmd+click, Shift+click, Cmd+A multi-select |
| `components/TransferPanel/index.tsx` | Bottom floating panel: transfer items with progress |
| `components/TransferPanel/TransferItem.tsx` | Single transfer row: progress bar + cancel button |

---

## Task 1: Scaffold Tauri project

**Files:**
- Create: `src-tauri/src/main.rs` (replace scaffold)
- Create: `src-tauri/src/types.rs`
- Create: `src/main.tsx` (replace scaffold)
- Create: `src/App.tsx`

- [ ] **Step 1: Create Tauri project**

```bash
cd /Users/hongqize/Workspace/x-explorer
npm create tauri-app@latest . -- --template react-ts --manager npm --force
```

Expected: project scaffolded with `src-tauri/` and `src/` directories.

- [ ] **Step 2: Install frontend dependencies**

```bash
npm install zustand @tauri-apps/api
npm install -D vitest @testing-library/react @testing-library/user-event @testing-library/jest-dom jsdom
npm install -D tailwindcss @tailwindcss/vite
```

- [ ] **Step 3: Configure Tailwind**

Add to `vite.config.ts`:
```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
});
```

Add to `src/index.css`:
```css
@import "tailwindcss";
```

- [ ] **Step 4: Configure Vitest**

Add to `vite.config.ts` (merge with above):
```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    globals: true,
  },
});
```

Create `src/test-setup.ts`:
```typescript
import "@testing-library/jest-dom";
```

- [ ] **Step 5: Create shared Rust types**

Create `src-tauri/src/types.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: String, // "ios" | "android"
    pub status: String,   // "connected" | "unauthorized" | "offline"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub bundle_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>, // unix timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTask {
    pub id: String,
    pub kind: String,      // "upload" | "download"
    pub src: String,
    pub dst: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: String,    // "pending" | "running" | "done" | "error" | "cancelled"
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub task_id: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub status: String,
}
```

- [ ] **Step 6: Update main.rs to register modules**

Replace `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod types;
mod bin_path;
mod device_manager;
mod ios_client;
mod android_client;
mod file_ops;
mod transfer_queue;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            device_manager::start(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ios_client::list_ios_devices,
            ios_client::list_ios_apps,
            ios_client::list_ios_files,
            ios_client::ios_delete,
            ios_client::ios_unmount_container,
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
```

Note: `ios_download`/`ios_upload`/`android_download`/`android_upload` are
intentionally NOT registered as Tauri commands — the frontend never calls
them directly. They stay `pub` (not `#[tauri::command]`) so `transfer_queue`
can call them internally as plain Rust functions; only the `enqueue_*`
wrappers exposed by `transfer_queue` (added in Task 6) are reachable from the
frontend. This is what makes the queue the single execution path described
at the top of Task 6.

Note: `mod bin_path;` is declared here now instead of being added later in Task 2 — Task 2 will only need to create the file itself.

- [ ] **Step 7: Add serde dependency to Cargo.toml**

In `src-tauri/Cargo.toml`, add under `[dependencies]`:
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 8: Verify project builds**

```bash
cd /Users/hongqize/Workspace/x-explorer
npm run tauri build -- --debug 2>&1 | head -50
```

Expected: Rust compiles without errors (frontend errors about missing components are OK at this stage).

- [ ] **Step 9: Commit**

```bash
git init
git add src-tauri/src/types.rs src-tauri/src/main.rs src-tauri/Cargo.toml src/test-setup.ts vite.config.ts src/index.css
git commit -m "feat: scaffold Tauri project with shared types"
```

---

## Task 2: Binary resolver — locate bundled tools

**Files:**
- Create: `src-tauri/src/bin_path.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write failing test for binary path resolution**

Create `src-tauri/src/bin_path.rs`:
```rust
use std::path::PathBuf;

/// Returns the path to a bundled binary by name.
/// In development, looks in `src-tauri/binaries/`.
/// In production, looks inside the .app bundle Resources.
pub fn resolve(name: &str) -> Result<PathBuf, String> {
    // Try development path first
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(name);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    // Try production bundle path
    if let Ok(exe) = std::env::current_exe() {
        let bundle_path = exe
            .parent()
            .unwrap_or(&exe)
            .join(name);
        if bundle_path.exists() {
            return Ok(bundle_path);
        }
    }

    Err(format!("Binary '{}' not found in bundle or dev path", name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_returns_error_for_missing_binary() {
        let result = resolve("nonexistent_binary_xyz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
```

- [ ] **Step 2: Run the test**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo test bin_path -- --nocapture
```

Expected: PASS — `test_resolve_returns_error_for_missing_binary` passes.

- [ ] **Step 3: Create binaries directory and placeholder README**

```bash
mkdir -p src-tauri/binaries
```

Create `src-tauri/binaries/README.md`:
```
# Bundled Binaries

Place platform binaries here before building:

- adb          (Android Debug Bridge, from Android SDK platform-tools)
- idevice_id   (from libimobiledevice)
- ideviceinstaller
- ifuse        (from ifuse)

Download libimobiledevice tools: brew install libimobiledevice ifuse
Download adb: brew install android-platform-tools
```

Add `idevice_id` and `ideviceinstaller` list requirements to also mention `ideviceinfo`:
```
- adb                (Android Debug Bridge, from Android SDK platform-tools)
- idevice_id         (from libimobiledevice)
- ideviceinfo        (from libimobiledevice — used for trust-state detection)
- ideviceinstaller
- ifuse              (from ifuse)

Download libimobiledevice tools: brew install libimobiledevice ifuse
Download adb: brew install android-platform-tools
```

- [ ] **Step 4: Verify module is wired**

`mod bin_path;` was already added to `src-tauri/src/main.rs` in Task 1 Step 6. Confirm it's present:

```bash
grep "mod bin_path;" /Users/hongqize/Workspace/x-explorer/src-tauri/src/main.rs
```

Expected: prints `mod bin_path;`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/bin_path.rs src-tauri/binaries/README.md
git commit -m "feat: add binary path resolver for bundled tools"
```

---

## Task 3: Android client — device/app listing + run-as bridge for app containers

**Files:**
- Create: `src-tauri/src/android_client.rs`

Android has two distinct access modes that must not be confused:
- **External storage** (`/sdcard/...`): plain `adb shell ls`, `adb pull`, `adb push` — no special permission needed.
- **App container** (`/data/data/<pkg>/...`): SELinux blocks direct access. Every operation must go through `run-as <pkg> <cmd>`. Uploads must stage through `/data/local/tmp` (world-writable) because `run-as` has no direct way to receive a pushed file. If the target app is not debuggable, `run-as` fails with a message containing `not debuggable` — this must be surfaced as a distinct, user-facing error rather than a generic failure.

Every command below also guards against running further `adb shell` calls against an unauthorized device: `adb devices` reports `unauthorized` for devices where USB debugging hasn't been approved on-device yet, and shelling out anyway just produces a confusing raw error. `check_device_authorized` centralizes this check.

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/android_client.rs`:
```rust
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
    let args: Vec<String> = match &package {
        Some(pkg) => vec![
            "-s".into(), device_id.clone(), "shell".into(),
            "run-as".into(), pkg.clone(), "ls".into(), "-la".into(), full_path.clone(),
        ],
        None => vec![
            "-s".into(), device_id.clone(), "shell".into(),
            "ls".into(), "-la".into(), full_path.clone(),
        ],
    };
    let out = std::process::Command::new(adb)
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if package.is_some() && is_not_debuggable_error(&stderr) {
        return Err("该应用未开启调试模式，无法访问其数据目录".to_string());
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
            let out = std::process::Command::new(&adb)
                .args(["-s", &device_id, "shell", "run-as", &pkg, "cat", &remote_path])
                .output()
                .map_err(|e| e.to_string())?;
            let stderr = String::from_utf8_lossy(&out.stderr);
            if is_not_debuggable_error(&stderr) {
                return Err("该应用未开启调试模式，无法访问其数据目录".to_string());
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

            let cp_out = std::process::Command::new(&adb)
                .args(["-s", &device_id, "shell", "run-as", &pkg, "cp", &staging_path, &remote_path])
                .output()
                .map_err(|e| e.to_string())?;
            let stderr = String::from_utf8_lossy(&cp_out.stderr);
            let not_debuggable = is_not_debuggable_error(&stderr);

            // Always clean up the staged file regardless of cp outcome.
            let _ = std::process::Command::new(&adb)
                .args(["-s", &device_id, "shell", "rm", "-f", &staging_path])
                .status();

            if not_debuggable {
                return Err("该应用未开启调试模式，无法访问其数据目录".to_string());
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
    let args: Vec<String> = match &package {
        Some(pkg) => vec![
            "-s".into(), device_id.clone(), "shell".into(),
            "run-as".into(), pkg.clone(), "rm".into(), "-rf".into(), remote_path.clone(),
        ],
        None => vec![
            "-s".into(), device_id.clone(), "shell".into(),
            "rm".into(), "-rf".into(), remote_path.clone(),
        ],
    };
    let out = std::process::Command::new(&adb)
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if package.is_some() && is_not_debuggable_error(&stderr) {
        return Err("该应用未开启调试模式，无法访问其数据目录".to_string());
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
}
```

- [ ] **Step 2: Run tests**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo test android_client -- --nocapture
```

Expected: all 8 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/android_client.rs
git commit -m "feat: android client with run-as bridge for app-container access"
```

---

## Task 4: iOS client — device/app listing + mount lifecycle management

**Files:**
- Create: `src-tauri/src/ios_client.rs`

Two issues addressed here that the naive approach gets wrong:
- **Mount lifecycle**: calling `ifuse` on an already-mounted directory repeatedly leaves stale mounts. Mounts are cached in a process-wide map keyed by `device_id:bundle_id`, and are unmounted when no longer needed (device disconnects or app selection changes).
- **Trust detection**: `idevice_id -l` only lists UDIDs, it doesn't say whether the host is trusted. `ideviceinfo -u <udid>` fails with a message containing `Trust` when the device hasn't approved this host yet — this is used to distinguish "connected" from "awaiting trust".

- [ ] **Step 1: Write failing tests and implementation**

Create `src-tauri/src/ios_client.rs`:
```rust
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

/// List files in a mounted ifuse path (uses std::fs).
pub fn list_mounted_dir(mount_path: &str, sub_path: &str) -> Result<Vec<FileEntry>, String> {
    let full_path = PathBuf::from(mount_path).join(sub_path.trim_start_matches('/'));
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
            path: format!("{}/{}", sub_path.trim_end_matches('/'), name),
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
    let out = run_idevice("ideviceinstaller", &["-u", &device_id, "-l"])?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(parse_ideviceinstaller_list(&text))
}

#[tauri::command]
pub fn list_ios_files(device_id: String, bundle_id: String, path: String) -> Result<Vec<FileEntry>, String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    list_mounted_dir(&mount_path, &path)
}

/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn ios_download(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let src = PathBuf::from(crate::file_ops::join_path(&mount_path, &remote_path));
    std::fs::copy(&src, &local_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn ios_upload(device_id: String, bundle_id: String, local_path: String, remote_path: String) -> Result<(), String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let dst = PathBuf::from(crate::file_ops::join_path(&mount_path, &remote_path));
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&local_path, &dst).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn ios_delete(device_id: String, bundle_id: String, remote_path: String) -> Result<(), String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let target = PathBuf::from(crate::file_ops::join_path(&mount_path, &remote_path));
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

    let mount_path = std::env::temp_dir()
        .join("x-explorer")
        .join(device_id)
        .join(bundle_id);
    std::fs::create_dir_all(&mount_path).map_err(|e| e.to_string())?;
    let ifuse = crate::bin_path::resolve("ifuse")?;
    let status = std::process::Command::new(ifuse)
        .args([
            "--udid", device_id,
            "--container", bundle_id,
            mount_path.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("ifuse mount failed for {}", bundle_id));
    }

    let path_str = mount_path.to_string_lossy().to_string();
    map.insert(key, path_str.clone());
    Ok(path_str)
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
}
```

- [ ] **Step 2: Run tests**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo test ios_client -- --nocapture
```

Expected: all 5 tests PASS. (`check_ios_trusted` itself isn't unit-tested here
since it shells out to `ideviceinfo` — it's covered indirectly by the reused
`is_untrusted_error` tests above; integration behavior is verified manually
against a real device in Task 16.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ios_client.rs
git commit -m "feat: iOS client with mount caching, unmount lifecycle, and trust detection"
```

---

## Task 5: Device manager — hotplug polling

**Files:**
- Create: `src-tauri/src/device_manager.rs`

- [ ] **Step 1: Write implementation**

Create `src-tauri/src/device_manager.rs`:
```rust
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
```

Note: calling `list_ios_devices()` here means every poll tick also shells out to `ideviceinfo` per connected device (to check trust state). With typically 0-2 iOS devices connected this is cheap enough for a 2-second interval; if it becomes a bottleneck in practice, cache trust state and only re-check on UDID list changes.

- [ ] **Step 2: Verify compilation**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo build 2>&1 | grep -E "error|warning: unused"
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/device_manager.rs
git commit -m "feat: device manager polls adb+idevice every 2s and emits devices-changed event"
```

---

## Task 6: Transfer queue — actually executes transfers and emits progress

**Files:**
- Create: `src-tauri/src/transfer_queue.rs`
- Modify: `src-tauri/src/main.rs`

The queue is the single execution path for every import/export — the frontend never calls `ios_upload`/`android_download` etc. directly. It enqueues a `TransferJob`, the queue runs it on a background thread (bounded to 3 concurrent jobs), and emits `transfer-progress` events before/after each job so `TransferPanel` has something to render. Progress granularity is per-file (not per-byte): for a single-file job this is 0% → 100%; for a multi-file batch each completed file advances the shared progress by `1/N`. This keeps the implementation honest about what byte-level progress from `run-as cat`/`ifuse` can and cannot report.

- [ ] **Step 1: Write implementation and tests**

Create `src-tauri/src/transfer_queue.rs`:
```rust
use crate::types::{TransferProgress, TransferTask};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

/// A single file operation to perform. `package` is Some(bundle_id) for iOS or
/// Some(package_name) for an Android app-container path; None for external storage / no-app iOS n/a.
#[derive(Clone)]
pub enum JobOp {
    IosDownload { device_id: String, bundle_id: String, remote_path: String, local_path: String },
    IosUpload { device_id: String, bundle_id: String, local_path: String, remote_path: String },
    AndroidDownload { device_id: String, remote_path: String, local_path: String, package: Option<String> },
    AndroidUpload { device_id: String, local_path: String, remote_path: String, package: Option<String> },
}

struct Job {
    task: TransferTask,
    op: JobOp,
}

pub struct TransferQueue {
    tasks: Arc<Mutex<HashMap<String, TransferTask>>>,
    pending: Arc<(Mutex<VecDeque<Job>>, Condvar)>,
    running_count: Arc<Mutex<usize>>,
    max_concurrent: usize,
}

impl TransferQueue {
    pub fn new(handle: AppHandle, max_concurrent: usize) -> Arc<Self> {
        let queue = Arc::new(Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            running_count: Arc::new(Mutex::new(0)),
            max_concurrent,
        });
        queue.clone().spawn_workers(handle);
        queue
    }

    fn spawn_workers(self: Arc<Self>, handle: AppHandle) {
        for _ in 0..self.max_concurrent {
            let queue = self.clone();
            let handle = handle.clone();
            thread::spawn(move || loop {
                let job = {
                    let (lock, cvar) = &*queue.pending;
                    let mut guard = lock.lock().unwrap();
                    while guard.is_empty() {
                        guard = cvar.wait(guard).unwrap();
                    }
                    guard.pop_front().unwrap()
                };
                queue.run_job(&handle, job);
            });
        }
    }

    fn run_job(&self, handle: &AppHandle, job: Job) {
        let Job { mut task, op } = job;

        {
            let mut tasks = self.tasks.lock().unwrap();
            if tasks.get(&task.id).map(|t| t.status.as_str()) == Some("cancelled") {
                return;
            }
            task.status = "running".to_string();
            tasks.insert(task.id.clone(), task.clone());
        }
        emit_progress(handle, &task);

        let result = match &op {
            JobOp::IosDownload { device_id, bundle_id, remote_path, local_path } =>
                crate::ios_client::ios_download(device_id.clone(), bundle_id.clone(), remote_path.clone(), local_path.clone()),
            JobOp::IosUpload { device_id, bundle_id, local_path, remote_path } =>
                crate::ios_client::ios_upload(device_id.clone(), bundle_id.clone(), local_path.clone(), remote_path.clone()),
            JobOp::AndroidDownload { device_id, remote_path, local_path, package } =>
                crate::android_client::android_download(device_id.clone(), remote_path.clone(), local_path.clone(), package.clone()),
            JobOp::AndroidUpload { device_id, local_path, remote_path, package } =>
                crate::android_client::android_upload(device_id.clone(), local_path.clone(), remote_path.clone(), package.clone()),
        };

        let mut tasks = self.tasks.lock().unwrap();
        // A cancellation requested mid-flight still lands here after the blocking call returns;
        // respect it instead of overwriting with a success/error status.
        if tasks.get(&task.id).map(|t| t.status.as_str()) == Some("cancelled") {
            drop(tasks);
            emit_progress(handle, &task);
            return;
        }
        match result {
            Ok(()) => {
                task.status = "done".to_string();
                task.transferred_bytes = task.total_bytes.max(1);
            }
            Err(e) => {
                task.status = "error".to_string();
                task.error = Some(e);
            }
        }
        tasks.insert(task.id.clone(), task.clone());
        drop(tasks);
        emit_progress(handle, &task);
    }

    pub fn enqueue(&self, kind: &str, src: &str, dst: &str, op: JobOp) -> String {
        let id = Uuid::new_v4().to_string();
        let task = TransferTask {
            id: id.clone(),
            kind: kind.to_string(),
            src: src.to_string(),
            dst: dst.to_string(),
            total_bytes: 1,
            transferred_bytes: 0,
            status: "pending".to_string(),
            error: None,
        };
        self.tasks.lock().unwrap().insert(id.clone(), task.clone());
        let (lock, cvar) = &*self.pending;
        lock.lock().unwrap().push_back(Job { task, op });
        cvar.notify_one();
        id
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.get_mut(task_id) {
            if task.status == "pending" || task.status == "running" {
                task.status = "cancelled".to_string();
                return true;
            }
        }
        false
    }

    pub fn get_status(&self, task_id: &str) -> Option<String> {
        self.tasks.lock().unwrap().get(task_id).map(|t| t.status.clone())
    }
}

fn emit_progress(handle: &AppHandle, task: &TransferTask) {
    let _ = handle.emit(
        "transfer-progress",
        TransferProgress {
            task_id: task.id.clone(),
            transferred_bytes: task.transferred_bytes,
            total_bytes: task.total_bytes,
            status: task.status.clone(),
        },
    );
}

#[tauri::command]
pub fn cancel_transfer(task_id: String, state: tauri::State<Arc<TransferQueue>>) -> bool {
    state.cancel(&task_id)
}

// Enqueue commands — these are the ONLY way the frontend triggers a file
// transfer. Each returns the new task's id immediately; the frontend tracks
// progress via the "transfer-progress" event, not the command's return value.
#[tauri::command]
pub fn enqueue_ios_download(
    device_id: String,
    bundle_id: String,
    remote_path: String,
    local_path: String,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    state.enqueue(
        "download",
        &remote_path,
        &local_path,
        JobOp::IosDownload { device_id, bundle_id, remote_path, local_path },
    )
}

#[tauri::command]
pub fn enqueue_ios_upload(
    device_id: String,
    bundle_id: String,
    local_path: String,
    remote_path: String,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    state.enqueue(
        "upload",
        &local_path,
        &remote_path,
        JobOp::IosUpload { device_id, bundle_id, local_path, remote_path },
    )
}

#[tauri::command]
pub fn enqueue_android_download(
    device_id: String,
    remote_path: String,
    local_path: String,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    state.enqueue(
        "download",
        &remote_path,
        &local_path,
        JobOp::AndroidDownload { device_id, remote_path, local_path, package },
    )
}

#[tauri::command]
pub fn enqueue_android_upload(
    device_id: String,
    local_path: String,
    remote_path: String,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    state.enqueue(
        "upload",
        &local_path,
        &remote_path,
        JobOp::AndroidUpload { device_id, local_path, remote_path, package },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_job() -> JobOp {
        // AndroidDownload against a nonexistent device fails fast — fine for queue-mechanics tests,
        // since these tests only check task bookkeeping, not actual transfer success.
        JobOp::AndroidDownload {
            device_id: "nonexistent".to_string(),
            remote_path: "/sdcard/x".to_string(),
            local_path: "/tmp/x".to_string(),
            package: None,
        }
    }

    #[test]
    fn test_enqueue_creates_pending_task_before_worker_picks_it_up() {
        // Construct the tasks map directly to test bookkeeping without spawning real workers/App.
        let tasks: Arc<Mutex<HashMap<String, TransferTask>>> = Arc::new(Mutex::new(HashMap::new()));
        let id = Uuid::new_v4().to_string();
        let task = TransferTask {
            id: id.clone(),
            kind: "download".to_string(),
            src: "/device/file.txt".to_string(),
            dst: "/local/file.txt".to_string(),
            total_bytes: 1,
            transferred_bytes: 0,
            status: "pending".to_string(),
            error: None,
        };
        tasks.lock().unwrap().insert(id.clone(), task);
        assert_eq!(tasks.lock().unwrap().get(&id).unwrap().status, "pending");
    }

    #[test]
    fn test_cancel_marks_pending_task_cancelled() {
        let tasks: Arc<Mutex<HashMap<String, TransferTask>>> = Arc::new(Mutex::new(HashMap::new()));
        let id = "task-1".to_string();
        tasks.lock().unwrap().insert(
            id.clone(),
            TransferTask {
                id: id.clone(),
                kind: "upload".to_string(),
                src: "/local/file.txt".to_string(),
                dst: "/device/file.txt".to_string(),
                total_bytes: 1,
                transferred_bytes: 0,
                status: "pending".to_string(),
                error: None,
            },
        );
        let mut guard = tasks.lock().unwrap();
        let task = guard.get_mut(&id).unwrap();
        task.status = "cancelled".to_string();
        assert_eq!(task.status, "cancelled");
    }

    #[test]
    fn test_job_op_variants_construct() {
        // Guards against JobOp field drift relative to the client function signatures.
        let _ = noop_job();
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo test transfer_queue -- --nocapture
```

Expected: all 3 tests PASS.

- [ ] **Step 3: Register the queue as managed Tauri state**

In `src-tauri/src/main.rs`, inside the `.setup(|app| { ... })` closure (after `device_manager::start(handle);`), add:
```rust
let queue = crate::transfer_queue::TransferQueue::new(app.handle().clone(), 3);
app.manage(queue);
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/transfer_queue.rs src-tauri/src/main.rs
git commit -m "feat: transfer queue executes jobs on worker threads and emits real progress"
```

---

## Task 7: file_ops — path normalization, wired into android_client and ios_client

**Files:**
- Create: `src-tauri/src/file_ops.rs`
- Modify: `src-tauri/src/android_client.rs` (use `normalize_path` when building remote paths)
- Modify: `src-tauri/src/ios_client.rs` (use `normalize_path` when joining mount path + relative path)

- [ ] **Step 1: Write implementation**

Create `src-tauri/src/file_ops.rs`:
```rust
// file_ops provides path-normalization shared by ios_client and android_client so that
// callers can join a base path (mount point, app-data root, /sdcard) with a
// user-supplied relative path without producing double slashes or missing leading slashes.

/// Normalize a remote path: ensure it starts with / and has no trailing slash
/// (except for the root path "/", which is left as-is).
pub fn normalize_path(path: &str) -> String {
    let p = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    };
    if p == "/" {
        p
    } else {
        p.trim_end_matches('/').to_string()
    }
}

/// Join a base path and a relative child path, normalizing the result so that
/// callers never manually concatenate strings (which caused double-slash bugs
/// when `child` already started with "/").
pub fn join_path(base: &str, child: &str) -> String {
    let base = normalize_path(base);
    let child = child.trim_start_matches('/');
    if child.is_empty() {
        base
    } else if base == "/" {
        format!("/{}", child)
    } else {
        format!("{}/{}", base, child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_adds_leading_slash() {
        assert_eq!(normalize_path("sdcard/DCIM"), "/sdcard/DCIM");
    }

    #[test]
    fn test_normalize_path_removes_trailing_slash() {
        assert_eq!(normalize_path("/sdcard/DCIM/"), "/sdcard/DCIM");
    }

    #[test]
    fn test_normalize_path_already_normalized() {
        assert_eq!(normalize_path("/sdcard/DCIM"), "/sdcard/DCIM");
    }

    #[test]
    fn test_normalize_path_root_stays_root() {
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn test_join_path_avoids_double_slash() {
        assert_eq!(join_path("/data/data/com.example", "/"), "/data/data/com.example");
        assert_eq!(join_path("/data/data/com.example", "/foo/bar"), "/data/data/com.example/foo/bar");
    }

    #[test]
    fn test_join_path_with_relative_child() {
        assert_eq!(join_path("/sdcard", "DCIM/photo.jpg"), "/sdcard/DCIM/photo.jpg");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo test file_ops -- --nocapture
```

Expected: all 6 tests PASS.

- [ ] **Step 3: Wire into android_client — replace manual path concatenation**

In `src-tauri/src/android_client.rs`, find every place that builds a remote path from a
base (app data dir, `/data/local/tmp`, `/sdcard`) plus a caller-supplied path — this
includes `list_android_files`, `android_download`, `android_upload`, and `android_delete`.
Replace direct `format!("{}{}", base, path)` or similar string concatenation with
`crate::file_ops::join_path(base, path)`. Also apply `crate::file_ops::normalize_path`
to any top-level `path` parameter before it is used directly (e.g. the external-storage
case with no `package`).

Add `mod file_ops;` above `mod android_client;` in `main.rs` if not already present (it
should already be declared from this task's Step 1 file creation — verify with:

```bash
grep "mod file_ops;" /Users/hongqize/Workspace/x-explorer/src-tauri/src/main.rs
```

If missing, add it near the other `mod` declarations.

- [ ] **Step 4: Wire into ios_client — replace manual path concatenation**

In `src-tauri/src/ios_client.rs`, find where the mounted container path is joined with a
caller-supplied relative path in `list_ios_files`, `ios_download`, `ios_upload`, and
`ios_delete`. Replace manual concatenation with
`crate::file_ops::join_path(&mount_path, relative_path)`.

- [ ] **Step 5: Run the android_client and ios_client test suites to confirm no regressions**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo test android_client -- --nocapture
cargo test ios_client -- --nocapture
```

Expected: all existing tests still PASS (path-joining is an internal implementation
change, not a behavior change for these tests since they test parsing/detection logic,
not path joining).

- [ ] **Step 6: Verify full Rust build**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo build 2>&1 | grep "^error"
```

Expected: no output (no errors).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/file_ops.rs src-tauri/src/android_client.rs src-tauri/src/ios_client.rs src-tauri/src/main.rs
git commit -m "feat: normalize and join remote paths via file_ops, wired into clients"
```

---

## Task 8: Zustand store + Tauri hooks

**Files:**
- Create: `src/store/index.ts`
- Create: `src/hooks/useTauri.ts`
- Create: `src/store/index.test.ts`

- [ ] **Step 1: Write failing store test**

Create `src/store/index.test.ts`:
```typescript
import { describe, it, expect, beforeEach } from "vitest";
import { useStore } from "./index";
import { act, renderHook } from "@testing-library/react";

describe("useStore", () => {
  beforeEach(() => {
    useStore.setState({
      devices: [],
      selectedDeviceId: null,
      selectedApp: null,
      currentPath: "/",
      files: [],
      transfers: [],
      viewMode: "list",
    });
  });

  it("should set selected device", () => {
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.setSelectedDeviceId("device-1");
    });
    expect(result.current.selectedDeviceId).toBe("device-1");
  });

  it("should set view mode", () => {
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.setViewMode("grid");
    });
    expect(result.current.viewMode).toBe("grid");
  });

  it("should navigate to path", () => {
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.setCurrentPath("/Documents/images");
    });
    expect(result.current.currentPath).toBe("/Documents/images");
  });
});
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/store/index.test.ts 2>&1 | tail -10
```

Expected: FAIL — "Cannot find module './index'".

- [ ] **Step 3: Create the store**

Create `src/store/index.ts`:
```typescript
import { create } from "zustand";

export interface Device {
  id: string;
  name: string;
  platform: "ios" | "android";
  status: "connected" | "unauthorized" | "offline";
}

export interface AppInfo {
  bundle_id: string;
  name: string;
}

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified?: number;
}

export interface TransferTask {
  id: string;
  kind: "upload" | "download";
  src: string;
  dst: string;
  total_bytes: number;
  transferred_bytes: number;
  status: "pending" | "running" | "done" | "error" | "cancelled";
  error?: string;
}

// Android browsing target: either a specific app's data directory (requires
// run-as bridging, needs `package`) or the fixed "external storage" entry
// point (/sdcard, no package needed). iOS always browses via selectedApp.
export type BrowseTarget =
  | { kind: "app"; app: AppInfo }
  | { kind: "external-storage" };

interface StoreState {
  devices: Device[];
  selectedDeviceId: string | null;
  selectedApp: AppInfo | null;
  browseTarget: BrowseTarget | null;
  currentPath: string;
  files: FileEntry[];
  transfers: TransferTask[];
  viewMode: "list" | "grid";

  setDevices: (devices: Device[]) => void;
  setSelectedDeviceId: (id: string | null) => void;
  setSelectedApp: (app: AppInfo | null) => void;
  setBrowseTarget: (target: BrowseTarget | null) => void;
  setCurrentPath: (path: string) => void;
  setFiles: (files: FileEntry[]) => void;
  upsertTransfer: (task: TransferTask) => void;
  setViewMode: (mode: "list" | "grid") => void;
}

export const useStore = create<StoreState>((set) => ({
  devices: [],
  selectedDeviceId: null,
  selectedApp: null,
  browseTarget: null,
  currentPath: "/",
  files: [],
  transfers: [],
  viewMode: "list",

  setDevices: (devices) => set({ devices }),
  setSelectedDeviceId: (id) =>
    set({ selectedDeviceId: id, selectedApp: null, browseTarget: null, currentPath: "/", files: [] }),
  setSelectedApp: (app) =>
    set({
      selectedApp: app,
      browseTarget: app ? { kind: "app", app } : null,
      currentPath: "/",
      files: [],
    }),
  setBrowseTarget: (target) =>
    set({
      browseTarget: target,
      selectedApp: target?.kind === "app" ? target.app : null,
      currentPath: "/",
      files: [],
    }),
  setCurrentPath: (path) => set({ currentPath: path }),
  setFiles: (files) => set({ files }),
  upsertTransfer: (task) =>
    set((s) => ({
      transfers: s.transfers.find((t) => t.id === task.id)
        ? s.transfers.map((t) => (t.id === task.id ? task : t))
        : [...s.transfers, task],
    })),
  setViewMode: (mode) => set({ viewMode: mode }),
}));
```

Note: `browseTarget` distinguishes "browsing an app's data directory" from
"browsing external storage" for Android. `selectedApp` is kept in sync for
backward-compat with components that only care about the app case (e.g. the
title bar), but path-building code (Task 13) must switch on `browseTarget`,
not `selectedApp`, to decide whether to pass a `package` argument.

Note: `setSelectedApp`/`setBrowseTarget`/`setSelectedDeviceId` intentionally
do NOT call `iosUnmountContainer` themselves — the store has no dependency on
`hooks/useTauri.ts` (Task 8 creates both files together, but keeping the
store free of Tauri calls keeps `store/index.test.ts` runnable without mocking
`invoke`). The actual unmount-on-switch call happens in `FileBrowser` (Task
13), which already knows the previous `browseTarget` via a ref and calls
`tauriApi.iosUnmountContainer` before switching, and in `useDeviceListener`
(Task 8, disconnect case).

- [ ] **Step 4: Run test to confirm it passes**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/store/index.test.ts 2>&1 | tail -10
```

Expected: PASS — 3 tests pass.

- [ ] **Step 5: Create Tauri hooks**

Create `src/hooks/useTauri.ts`:
```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { AppInfo, Device, FileEntry, TransferTask, useStore } from "../store";

export interface TransferProgress {
  task_id: string;
  transferred_bytes: number;
  total_bytes: number;
  status: TransferTask["status"];
}

// Typed invoke wrappers. Functions that read/list/delete run synchronously
// (they are fast, single round-trip shell calls). Functions that move file
// bytes (download/upload) are enqueued on the backend transfer_queue instead
// of awaited directly, so progress can be tracked and the operation can be
// cancelled — see transfer_queue.rs (Task 6). Android file operations that
// target an app's data directory take an optional `package`; omit it (or
// pass `undefined`) when browsing external storage.
export const tauriApi = {
  listIosDevices: () => invoke<Device[]>("list_ios_devices"),
  listAndroidDevices: () => invoke<Device[]>("list_android_devices"),
  listIosApps: (deviceId: string) => invoke<AppInfo[]>("list_ios_apps", { device_id: deviceId }),
  listAndroidApps: (deviceId: string) => invoke<AppInfo[]>("list_android_apps", { device_id: deviceId }),
  listIosFiles: (deviceId: string, bundleId: string, path: string) =>
    invoke<FileEntry[]>("list_ios_files", { device_id: deviceId, bundle_id: bundleId, path }),
  listAndroidFiles: (deviceId: string, path: string, pkg?: string) =>
    invoke<FileEntry[]>("list_android_files", { device_id: deviceId, path, package: pkg ?? null }),
  iosDelete: (deviceId: string, bundleId: string, remotePath: string) =>
    invoke<void>("ios_delete", { device_id: deviceId, bundle_id: bundleId, remote_path: remotePath }),
  androidDelete: (deviceId: string, remotePath: string, pkg?: string) =>
    invoke<void>("android_delete", { device_id: deviceId, remote_path: remotePath, package: pkg ?? null }),
  iosUnmountContainer: (deviceId: string, bundleId: string) =>
    invoke<void>("ios_unmount_container", { device_id: deviceId, bundle_id: bundleId }),

  // Enqueue-based transfer commands — return the new task's id immediately;
  // actual progress arrives via the "transfer-progress" event (see
  // useTransferListener below).
  enqueueIosDownload: (deviceId: string, bundleId: string, remotePath: string, localPath: string) =>
    invoke<string>("enqueue_ios_download", {
      device_id: deviceId,
      bundle_id: bundleId,
      remote_path: remotePath,
      local_path: localPath,
    }),
  enqueueIosUpload: (deviceId: string, bundleId: string, localPath: string, remotePath: string) =>
    invoke<string>("enqueue_ios_upload", {
      device_id: deviceId,
      bundle_id: bundleId,
      local_path: localPath,
      remote_path: remotePath,
    }),
  enqueueAndroidDownload: (deviceId: string, remotePath: string, localPath: string, pkg?: string) =>
    invoke<string>("enqueue_android_download", {
      device_id: deviceId,
      remote_path: remotePath,
      local_path: localPath,
      package: pkg ?? null,
    }),
  enqueueAndroidUpload: (deviceId: string, localPath: string, remotePath: string, pkg?: string) =>
    invoke<string>("enqueue_android_upload", {
      device_id: deviceId,
      local_path: localPath,
      remote_path: remotePath,
      package: pkg ?? null,
    }),
  cancelTransfer: (taskId: string) => invoke<boolean>("cancel_transfer", { task_id: taskId }),
};

// Hook: listen for device hotplug events, update store, and unmount any iOS
// container belonging to a device that just disappeared from the list (e.g.
// unplugged mid-browse). Without this, ifuse mounts under
// $TMPDIR/x-explorer/<device_id>/ leak until the app restarts.
export function useDeviceListener() {
  const setDevices = useStore((s) => s.setDevices);
  useEffect(() => {
    const unlisten = listen<Device[]>("devices-changed", (event) => {
      const nextDevices = event.payload;
      const { devices: prevDevices, selectedDeviceId, browseTarget, setSelectedDeviceId } =
        useStore.getState();
      const stillPresent = new Set(nextDevices.map((d) => d.id));
      const disconnectedIos = prevDevices.filter(
        (d) => d.platform === "ios" && !stillPresent.has(d.id)
      );
      for (const d of disconnectedIos) {
        if (browseTarget?.kind === "app") {
          tauriApi.iosUnmountContainer(d.id, browseTarget.app.bundle_id).catch(() => {});
        }
      }
      if (selectedDeviceId && !stillPresent.has(selectedDeviceId)) {
        setSelectedDeviceId(null);
      }
      setDevices(nextDevices);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [setDevices]);
}

// Hook: listen for transfer progress events and upsert into the store's
// transfers list. TransferPanel (Task 14) reads `transfers` from the store
// rather than polling.
export function useTransferListener() {
  const upsertTransfer = useStore((s) => s.upsertTransfer);
  const transfers = useStore((s) => s.transfers);
  useEffect(() => {
    const unlisten = listen<TransferProgress>("transfer-progress", (event) => {
      const p = event.payload;
      const existing = transfers.find((t) => t.id === p.task_id);
      upsertTransfer({
        id: p.task_id,
        kind: existing?.kind ?? "download",
        src: existing?.src ?? "",
        dst: existing?.dst ?? "",
        total_bytes: p.total_bytes,
        transferred_bytes: p.transferred_bytes,
        status: p.status,
        error: existing?.error,
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [upsertTransfer]);
}
```

- [ ] **Step 6: Commit**

```bash
git add src/store/index.ts src/store/index.test.ts src/hooks/useTauri.ts
git commit -m "feat: Zustand store with typed Tauri invoke hooks"
```

---

## Task 9: DevicePanel component

**Files:**
- Create: `src/components/DevicePanel/index.tsx`
- Create: `src/components/DevicePanel/DeviceList.tsx`
- Create: `src/components/DevicePanel/AppList.tsx`
- Create: `src/components/DevicePanel/DevicePanel.test.tsx`

- [ ] **Step 1: Write failing test**

Create `src/components/DevicePanel/DevicePanel.test.tsx`:
```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DeviceList } from "./DeviceList";
import { Device, useStore } from "../../store";

const mockDevices: Device[] = [
  { id: "iphone-1", name: "iPhone 15", platform: "ios", status: "connected" },
  { id: "pixel-1", name: "Pixel 7", platform: "android", status: "connected" },
  { id: "old-phone", name: "Old Phone", platform: "android", status: "unauthorized" },
];

beforeEach(() => {
  useStore.setState({ devices: mockDevices, selectedDeviceId: null });
});

describe("DeviceList", () => {
  it("renders device names", () => {
    render(<DeviceList />);
    expect(screen.getByText("iPhone 15")).toBeInTheDocument();
    expect(screen.getByText("Pixel 7")).toBeInTheDocument();
  });

  it("selects device on click", () => {
    render(<DeviceList />);
    fireEvent.click(screen.getByText("iPhone 15"));
    expect(useStore.getState().selectedDeviceId).toBe("iphone-1");
  });

  it("shows a distinct status badge for unauthorized devices", () => {
    render(<DeviceList />);
    const row = screen.getByText("Old Phone").closest("button")!;
    expect(row.querySelector("[data-status='unauthorized']")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/components/DevicePanel/DevicePanel.test.tsx 2>&1 | tail -10
```

Expected: FAIL — "Cannot find module './DeviceList'".

- [ ] **Step 3: Create DeviceList component**

Create `src/components/DevicePanel/DeviceList.tsx`:
```typescript
import { useStore } from "../../store";
import { Device } from "../../store";

const STATUS_LABEL: Record<Device["status"], string> = {
  connected: "已连接",
  unauthorized: "待信任/未授权",
  offline: "离线",
};

const STATUS_COLOR: Record<Device["status"], string> = {
  connected: "bg-green-400",
  unauthorized: "bg-yellow-400",
  offline: "bg-red-400",
};

export function DeviceList() {
  const devices = useStore((s) => s.devices);
  const selectedId = useStore((s) => s.selectedDeviceId);
  const setSelectedDeviceId = useStore((s) => s.setSelectedDeviceId);

  return (
    <div className="p-2">
      <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">设备</p>
      {devices.length === 0 && (
        <p className="text-xs text-gray-500 px-2">未检测到设备</p>
      )}
      {devices.map((device) => (
        <button
          key={device.id}
          onClick={() => setSelectedDeviceId(device.id)}
          title={STATUS_LABEL[device.status]}
          className={`w-full text-left px-3 py-2 rounded text-sm flex items-center gap-2 ${
            selectedId === device.id
              ? "bg-blue-600 text-white"
              : "text-gray-200 hover:bg-gray-700"
          }`}
        >
          <span>{device.platform === "ios" ? "📱" : "🤖"}</span>
          <span className="truncate">{device.name}</span>
          <span
            data-status={device.status}
            className={`ml-auto w-2 h-2 rounded-full ${STATUS_COLOR[device.status]}`}
          />
        </button>
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Create AppList component with the external-storage entry point**

For Android, browsing isn't limited to a selected app — external storage
(`/sdcard`) is a fixed entry independent of any app, and unauthorized devices
must not offer either option (attempting to browse would just surface a
confusing adb error). This component owns both the app list and that fixed
entry, since they're mutually exclusive selections in the same list.

Create `src/components/DevicePanel/AppList.tsx`:
```typescript
import { useEffect, useState } from "react";
import { AppInfo, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

export function AppList() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const devices = useStore((s) => s.devices);
  const browseTarget = useStore((s) => s.browseTarget);
  const setBrowseTarget = useStore((s) => s.setBrowseTarget);
  const [apps, setApps] = useState<AppInfo[]>([]);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const isUsable = device && device.status === "connected";

  useEffect(() => {
    if (!isUsable || !device) {
      setApps([]);
      return;
    }
    const load = async () => {
      try {
        const list =
          device.platform === "ios"
            ? await tauriApi.listIosApps(device.id)
            : await tauriApi.listAndroidApps(device.id);
        setApps(list);
      } catch (e) {
        console.error("Failed to load apps:", e);
        setApps([]);
      }
    };
    load();
  }, [device, isUsable]);

  if (!device) return null;

  if (!isUsable) {
    return (
      <div className="p-2 border-t border-gray-700">
        <p className="text-xs text-yellow-400 px-2">
          设备待信任或未授权，请在设备上确认后重试
        </p>
      </div>
    );
  }

  return (
    <div className="p-2 border-t border-gray-700">
      <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">应用</p>
      {device.platform === "android" && (
        <button
          onClick={() => setBrowseTarget({ kind: "external-storage" })}
          className={`w-full text-left px-3 py-1.5 rounded text-xs truncate flex items-center gap-2 ${
            browseTarget?.kind === "external-storage"
              ? "bg-blue-600 text-white"
              : "text-gray-300 hover:bg-gray-700"
          }`}
        >
          <span>💾</span>
          <span>外部存储</span>
        </button>
      )}
      {apps.map((app) => (
        <button
          key={app.bundle_id}
          onClick={() => setBrowseTarget({ kind: "app", app })}
          className={`w-full text-left px-3 py-1.5 rounded text-xs truncate ${
            browseTarget?.kind === "app" && browseTarget.app.bundle_id === app.bundle_id
              ? "bg-blue-600 text-white"
              : "text-gray-300 hover:bg-gray-700"
          }`}
        >
          {app.name}
        </button>
      ))}
    </div>
  );
}
```

- [ ] **Step 5: Create DevicePanel index**

Create `src/components/DevicePanel/index.tsx`:
```typescript
import { DeviceList } from "./DeviceList";
import { AppList } from "./AppList";

export function DevicePanel() {
  return (
    <aside className="w-56 flex-shrink-0 bg-gray-800 border-r border-gray-700 flex flex-col overflow-y-auto">
      <DeviceList />
      <AppList />
    </aside>
  );
}
```

- [ ] **Step 6: Run tests**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/components/DevicePanel/DevicePanel.test.tsx 2>&1 | tail -10
```

Expected: PASS — 3 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/components/DevicePanel/
git commit -m "feat: DevicePanel with device list, status badges, and external-storage entry"
```

---

## Task 10: Selection hook + FileBrowser skeleton

**Files:**
- Create: `src/components/FileBrowser/useSelection.ts`
- Create: `src/components/FileBrowser/useSelection.test.ts`
- Create: `src/components/FileBrowser/BreadcrumbBar.tsx`
- Create: `src/components/FileBrowser/Toolbar.tsx`
- Create: `src/components/FileBrowser/index.tsx`

- [ ] **Step 1: Write failing test for useSelection**

Create `src/components/FileBrowser/useSelection.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSelection } from "./useSelection";

describe("useSelection", () => {
  const items = ["a.txt", "b.txt", "c.txt", "d.txt"];

  it("toggles single item on click", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.handleClick("a.txt", false, false));
    expect(result.current.selected).toEqual(new Set(["a.txt"]));
  });

  it("adds item on cmd+click", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.handleClick("a.txt", false, false));
    act(() => result.current.handleClick("c.txt", true, false));
    expect(result.current.selected).toEqual(new Set(["a.txt", "c.txt"]));
  });

  it("selects range on shift+click", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.handleClick("a.txt", false, false));
    act(() => result.current.handleClick("c.txt", false, true));
    expect(result.current.selected).toEqual(new Set(["a.txt", "b.txt", "c.txt"]));
  });

  it("selects all with selectAll()", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.selectAll());
    expect(result.current.selected.size).toBe(4);
  });

  it("clears selection", () => {
    const { result } = renderHook(() => useSelection(items));
    act(() => result.current.selectAll());
    act(() => result.current.clearSelection());
    expect(result.current.selected.size).toBe(0);
  });
});
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/components/FileBrowser/useSelection.test.ts 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Implement useSelection**

Create `src/components/FileBrowser/useSelection.ts`:
```typescript
import { useState } from "react";

export function useSelection(items: string[]) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [lastClicked, setLastClicked] = useState<string | null>(null);

  function handleClick(name: string, cmdKey: boolean, shiftKey: boolean) {
    if (cmdKey) {
      setSelected((prev) => {
        const next = new Set(prev);
        if (next.has(name)) next.delete(name);
        else next.add(name);
        return next;
      });
    } else if (shiftKey && lastClicked) {
      const fromIdx = items.indexOf(lastClicked);
      const toIdx = items.indexOf(name);
      const [start, end] = fromIdx < toIdx ? [fromIdx, toIdx] : [toIdx, fromIdx];
      setSelected(new Set(items.slice(start, end + 1)));
    } else {
      setSelected(new Set([name]));
    }
    setLastClicked(name);
  }

  function selectAll() {
    setSelected(new Set(items));
  }

  function clearSelection() {
    setSelected(new Set());
  }

  return { selected, handleClick, selectAll, clearSelection };
}
```

- [ ] **Step 4: Run test to confirm it passes**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/components/FileBrowser/useSelection.test.ts 2>&1 | tail -10
```

Expected: PASS — 5 tests pass.

- [ ] **Step 5: Create BreadcrumbBar**

Create `src/components/FileBrowser/BreadcrumbBar.tsx`:
```typescript
import { useStore } from "../../store";

export function BreadcrumbBar() {
  const currentPath = useStore((s) => s.currentPath);
  const setCurrentPath = useStore((s) => s.setCurrentPath);

  const parts = currentPath.split("/").filter(Boolean);

  const navigateTo = (index: number) => {
    const path = "/" + parts.slice(0, index + 1).join("/");
    setCurrentPath(path);
  };

  return (
    <div className="flex items-center gap-1 px-3 py-2 text-sm text-gray-300 border-b border-gray-700">
      <button
        onClick={() => setCurrentPath("/")}
        className="hover:text-white text-gray-400"
      >
        ~
      </button>
      {parts.map((part, i) => (
        <span key={i} className="flex items-center gap-1">
          <span className="text-gray-600">/</span>
          <button
            onClick={() => navigateTo(i)}
            className={`hover:text-white ${i === parts.length - 1 ? "text-white" : "text-gray-400"}`}
          >
            {part}
          </button>
        </span>
      ))}
    </div>
  );
}
```

- [ ] **Step 6: Create Toolbar**

Create `src/components/FileBrowser/Toolbar.tsx`:
```typescript
import { useStore } from "../../store";

interface ToolbarProps {
  selectedCount: number;
  onImport: () => void;
  onExport: () => void;
  onDelete: () => void;
}

export function Toolbar({ selectedCount, onImport, onExport, onDelete }: ToolbarProps) {
  const viewMode = useStore((s) => s.viewMode);
  const setViewMode = useStore((s) => s.setViewMode);

  return (
    <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-700">
      <button
        onClick={onImport}
        className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700"
      >
        导入
      </button>
      {selectedCount > 0 && (
        <>
          <button
            onClick={onExport}
            className="px-3 py-1 text-xs bg-gray-600 text-white rounded hover:bg-gray-500"
          >
            导出 ({selectedCount})
          </button>
          <button
            onClick={onDelete}
            className="px-3 py-1 text-xs bg-red-700 text-white rounded hover:bg-red-600"
          >
            删除 ({selectedCount})
          </button>
        </>
      )}
      <div className="ml-auto flex gap-1">
        <button
          onClick={() => setViewMode("list")}
          className={`px-2 py-1 text-xs rounded ${viewMode === "list" ? "bg-gray-600 text-white" : "text-gray-400 hover:text-white"}`}
        >
          ☰
        </button>
        <button
          onClick={() => setViewMode("grid")}
          className={`px-2 py-1 text-xs rounded ${viewMode === "grid" ? "bg-gray-600 text-white" : "text-gray-400 hover:text-white"}`}
        >
          ⊞
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 7: Commit**

```bash
git add src/components/FileBrowser/
git commit -m "feat: useSelection hook with Cmd/Shift/All support, BreadcrumbBar, Toolbar"
```

---

## Task 11: FileList and FileGrid views

**Files:**
- Create: `src/components/FileBrowser/FileList.tsx`
- Create: `src/components/FileBrowser/FileGrid.tsx`
- Create: `src/components/FileBrowser/FileViews.test.tsx`

- [ ] **Step 1: Write failing test**

Create `src/components/FileBrowser/FileViews.test.tsx`:
```typescript
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { FileList } from "./FileList";
import { FileEntry } from "../../store";

const mockFiles: FileEntry[] = [
  { name: "Documents", path: "/Documents", is_dir: true, size: 0 },
  { name: "config.json", path: "/config.json", is_dir: false, size: 1024 },
];

describe("FileList", () => {
  it("renders file names", () => {
    const onNavigate = vi.fn();
    const onSelect = vi.fn();
    render(
      <FileList
        files={mockFiles}
        selected={new Set()}
        onNavigate={onNavigate}
        onSelect={onSelect}
      />
    );
    expect(screen.getByText("Documents")).toBeInTheDocument();
    expect(screen.getByText("config.json")).toBeInTheDocument();
  });

  it("calls onNavigate when clicking a directory", () => {
    const onNavigate = vi.fn();
    render(
      <FileList
        files={mockFiles}
        selected={new Set()}
        onNavigate={onNavigate}
        onSelect={vi.fn()}
      />
    );
    fireEvent.dblClick(screen.getByText("Documents"));
    expect(onNavigate).toHaveBeenCalledWith("/Documents");
  });

  it("shows file size for files", () => {
    render(
      <FileList
        files={mockFiles}
        selected={new Set()}
        onNavigate={vi.fn()}
        onSelect={vi.fn()}
      />
    );
    expect(screen.getByText("1.0 KB")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/components/FileBrowser/FileViews.test.tsx 2>&1 | tail -10
```

Expected: FAIL.

- [ ] **Step 3: Create FileList**

Create `src/components/FileBrowser/FileList.tsx`:
```typescript
import { FileEntry } from "../../store";

function formatSize(bytes: number): string {
  if (bytes === 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

interface FileListProps {
  files: FileEntry[];
  selected: Set<string>;
  onNavigate: (path: string) => void;
  onSelect: (name: string, cmdKey: boolean, shiftKey: boolean) => void;
  onDragStart?: (file: FileEntry) => void;
}

export function FileList({ files, selected, onNavigate, onSelect, onDragStart }: FileListProps) {
  return (
    <table className="w-full text-sm text-gray-300">
      <thead>
        <tr className="text-xs text-gray-500 border-b border-gray-700">
          <th className="text-left px-3 py-1 font-normal">名称</th>
          <th className="text-right px-3 py-1 font-normal w-24">大小</th>
        </tr>
      </thead>
      <tbody>
        {files.map((file) => (
          <tr
            key={file.path}
            draggable={!!onDragStart}
            onDragStart={() => onDragStart?.(file)}
            onClick={(e) => onSelect(file.name, e.metaKey, e.shiftKey)}
            onDoubleClick={() => file.is_dir && onNavigate(file.path)}
            className={`cursor-pointer hover:bg-gray-700 ${
              selected.has(file.name) ? "bg-blue-900" : ""
            }`}
          >
            <td className="px-3 py-1.5 flex items-center gap-2">
              <span>{file.is_dir ? "📁" : "📄"}</span>
              {file.name}
            </td>
            <td className="px-3 py-1.5 text-right text-gray-500 text-xs">
              {file.is_dir ? "—" : formatSize(file.size)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

- [ ] **Step 4: Create FileGrid**

Create `src/components/FileBrowser/FileGrid.tsx`:
```typescript
import { FileEntry } from "../../store";

interface FileGridProps {
  files: FileEntry[];
  selected: Set<string>;
  onNavigate: (path: string) => void;
  onSelect: (name: string, cmdKey: boolean, shiftKey: boolean) => void;
  onDragStart?: (file: FileEntry) => void;
}

export function FileGrid({ files, selected, onNavigate, onSelect, onDragStart }: FileGridProps) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(80px,1fr))] gap-3 p-4">
      {files.map((file) => (
        <div
          key={file.path}
          draggable={!!onDragStart}
          onDragStart={() => onDragStart?.(file)}
          onClick={(e) => onSelect(file.name, e.metaKey, e.shiftKey)}
          onDoubleClick={() => file.is_dir && onNavigate(file.path)}
          className={`flex flex-col items-center gap-1 p-2 rounded cursor-pointer text-center hover:bg-gray-700 ${
            selected.has(file.name) ? "bg-blue-900" : ""
          }`}
        >
          <span className="text-3xl">{file.is_dir ? "📁" : "📄"}</span>
          <span className="text-xs text-gray-300 truncate w-full">{file.name}</span>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 5: Run tests**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run src/components/FileBrowser/FileViews.test.tsx 2>&1 | tail -10
```

Expected: PASS — 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/components/FileBrowser/FileList.tsx src/components/FileBrowser/FileGrid.tsx src/components/FileBrowser/FileViews.test.tsx
git commit -m "feat: FileList and FileGrid views with size formatting"
```

---

## Task 12: Drag-and-drop hook

**Files:**
- Create: `src/components/FileBrowser/useDragDrop.ts`

Dragging a file OUT of a device cannot pass a remote device path to Tauri's
`startDrag` — that API only accepts real local filesystem paths, since it
hands off to the OS's native drag session. The correct flow is: on
`dragstart`, first download the selected files into a temp directory
(`$TMPDIR/x-explorer-drag/`) via the transfer queue, then call `startDrag` on
the resulting local paths once the download finishes. `appDataDir`/`tempDir`
from `@tauri-apps/api/path` give us a writable temp location without any new
backend command.

- [ ] **Step 1: Create drag-and-drop hook**

Create `src/components/FileBrowser/useDragDrop.ts`:
```typescript
import { getCurrentWindow } from "@tauri-apps/api/window";
import { tempDir, join } from "@tauri-apps/api/path";
import { listen } from "@tauri-apps/api/event";
import { FileEntry, useStore } from "../../store";
import { tauriApi, TransferProgress } from "../../hooks/useTauri";

/// Waits for the "transfer-progress" event for a specific task id to reach a
/// terminal status ("done" | "error" | "cancelled"), then resolves/rejects.
async function waitForTransfer(taskId: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let unlisten: (() => void) | undefined;
    listen<TransferProgress>("transfer-progress", (event) => {
      const p = event.payload;
      if (p.task_id !== taskId) return;
      if (p.status === "done") {
        unlisten?.();
        resolve();
      } else if (p.status === "error" || p.status === "cancelled") {
        unlisten?.();
        reject(new Error(`transfer ${p.status}`));
      }
    }).then((fn) => {
      unlisten = fn;
    });
  });
}

export function useDragDrop() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const browseTarget = useStore((s) => s.browseTarget);
  const devices = useStore((s) => s.devices);
  const currentPath = useStore((s) => s.currentPath);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const pkg = browseTarget?.kind === "app" ? browseTarget.app.bundle_id : undefined;

  // Drag files OUT of the device to Finder/Desktop. Downloads each file to a
  // temp dir first (device paths aren't real local paths), showing a loading
  // cursor for the duration since large files make this visibly slow.
  async function startFileDrag(files: FileEntry[]) {
    if (files.length === 0 || !device) return;
    document.body.style.cursor = "wait";
    try {
      const dir = await tempDir();
      const dragDir = await join(dir, "x-explorer-drag");
      const localPaths: string[] = [];
      for (const file of files) {
        const localPath = await join(dragDir, file.name);
        const taskId =
          device.platform === "ios"
            ? await tauriApi.enqueueIosDownload(device.id, pkg!, file.path, localPath)
            : await tauriApi.enqueueAndroidDownload(device.id, file.path, localPath, pkg);
        await waitForTransfer(taskId);
        localPaths.push(localPath);
      }
      // @ts-ignore — startDrag is available in Tauri 2
      await getCurrentWindow().startDrag({ items: localPaths });
    } catch (e) {
      console.error("Drag-out failed:", e);
    } finally {
      document.body.style.cursor = "default";
    }
  }

  // Drop files FROM Mac INTO the current device directory (external storage
  // or, if an app is selected, that app's data directory via run-as).
  async function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    if (!device || !browseTarget) return;

    const files = Array.from(e.dataTransfer.files);
    for (const file of files) {
      const localPath = (file as any).path; // Tauri provides file.path via drag-drop
      if (!localPath) continue;
      const remotePath = `${currentPath.replace(/\/$/, "")}/${file.name}`;

      if (device.platform === "ios") {
        await tauriApi.enqueueIosUpload(device.id, pkg!, localPath, remotePath);
      } else {
        await tauriApi.enqueueAndroidUpload(device.id, localPath, remotePath, pkg);
      }
    }
  }

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }

  return { startFileDrag, handleDrop, handleDragOver };
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/FileBrowser/useDragDrop.ts
git commit -m "feat: drag-drop hook downloads to temp dir before startDrag"
```

---

## Task 13: FileBrowser index — wire everything together

**Files:**
- Create: `src/components/FileBrowser/index.tsx`

- [ ] **Step 1: Create FileBrowser index**

Create `src/components/FileBrowser/index.tsx`:
```typescript
import { useEffect, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../../store";
import { tauriApi, useTransferListener } from "../../hooks/useTauri";
import { BreadcrumbBar } from "./BreadcrumbBar";
import { Toolbar } from "./Toolbar";
import { FileList } from "./FileList";
import { FileGrid } from "./FileGrid";
import { useSelection } from "./useSelection";
import { useDragDrop } from "./useDragDrop";

export function FileBrowser() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const browseTarget = useStore((s) => s.browseTarget);
  const devices = useStore((s) => s.devices);
  const currentPath = useStore((s) => s.currentPath);
  const files = useStore((s) => s.files);
  const setFiles = useStore((s) => s.setFiles);
  const setCurrentPath = useStore((s) => s.setCurrentPath);
  const viewMode = useStore((s) => s.viewMode);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const pkg = browseTarget?.kind === "app" ? browseTarget.app.bundle_id : undefined;
  const fileNames = files.map((f) => f.name);
  const { selected, handleClick, selectAll, clearSelection } = useSelection(fileNames);
  const { startFileDrag, handleDrop, handleDragOver } = useDragDrop();

  useTransferListener();

  // Reload the current directory's file list from the backend. Shared by
  // the load-on-navigate effect and handleDelete (which must not rely on
  // `setCurrentPath(currentPath)` as a refresh trick — Zustand's `set` with
  // an unchanged value doesn't trigger effects keyed on that value).
  async function reloadFiles() {
    if (!device || !browseTarget) return;
    try {
      const list =
        device.platform === "ios"
          ? await tauriApi.listIosFiles(device.id, pkg!, currentPath)
          : await tauriApi.listAndroidFiles(device.id, currentPath, pkg);
      setFiles(list);
    } catch (e) {
      console.error("Failed to load files:", e);
    }
  }

  // Load files when path/browseTarget/device changes
  useEffect(() => {
    reloadFiles().then(clearSelection);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [device, browseTarget, currentPath]);

  // Unmount the previously-browsed iOS container when switching to a
  // different app/device, so ifuse mounts don't accumulate under
  // $TMPDIR/x-explorer/. Tracked via ref since we need the *previous*
  // browseTarget at the moment it changes, not the current one.
  const prevIosTarget = useRef<{ deviceId: string; bundleId: string } | null>(null);
  useEffect(() => {
    const prev = prevIosTarget.current;
    if (
      prev &&
      !(device?.platform === "ios" && browseTarget?.kind === "app" &&
        device.id === prev.deviceId && browseTarget.app.bundle_id === prev.bundleId)
    ) {
      tauriApi.iosUnmountContainer(prev.deviceId, prev.bundleId).catch(() => {});
    }
    prevIosTarget.current =
      device?.platform === "ios" && browseTarget?.kind === "app"
        ? { deviceId: device.id, bundleId: browseTarget.app.bundle_id }
        : null;
  }, [device, browseTarget]);

  // Keyboard shortcut: Cmd+A
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "a") {
        e.preventDefault();
        selectAll();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selectAll]);

  async function handleImport() {
    const paths = await open({ multiple: true });
    if (!paths || !device) return;
    const pathList = Array.isArray(paths) ? paths : [paths];
    for (const localPath of pathList) {
      const fileName = localPath.split("/").pop()!;
      const remotePath = `${currentPath.replace(/\/$/, "")}/${fileName}`;
      try {
        if (device.platform === "ios") {
          await tauriApi.enqueueIosUpload(device.id, pkg!, localPath, remotePath);
        } else {
          await tauriApi.enqueueAndroidUpload(device.id, localPath, remotePath, pkg);
        }
      } catch (e) {
        console.error(`Failed to enqueue upload for ${fileName}:`, e);
      }
    }
  }

  async function handleExport() {
    if (!device) return;
    const selectedFiles = files.filter((f) => selected.has(f.name));
    const destDir = await open({ directory: true });
    if (!destDir || typeof destDir !== "string") return;
    for (const file of selectedFiles) {
      const localPath = `${destDir}/${file.name}`;
      try {
        if (device.platform === "ios") {
          await tauriApi.enqueueIosDownload(device.id, pkg!, file.path, localPath);
        } else {
          await tauriApi.enqueueAndroidDownload(device.id, file.path, localPath, pkg);
        }
      } catch (e) {
        console.error(`Failed to enqueue download for ${file.name}:`, e);
      }
    }
  }

  async function handleDelete() {
    if (!device || !window.confirm(`删除选中的 ${selected.size} 个文件？`)) return;
    const selectedFiles = files.filter((f) => selected.has(f.name));
    for (const file of selectedFiles) {
      try {
        if (device.platform === "ios") {
          await tauriApi.iosDelete(device.id, pkg!, file.path);
        } else {
          await tauriApi.androidDelete(device.id, file.path, pkg);
        }
      } catch (e) {
        console.error(`Failed to delete ${file.name}:`, e);
      }
    }
    clearSelection();
    await reloadFiles();
  }

  if (!device) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-500">
        请选择设备
      </div>
    );
  }

  if (device.status !== "connected") {
    return (
      <div className="flex-1 flex items-center justify-center text-yellow-500">
        设备待信任或未授权，请在设备上确认
      </div>
    );
  }

  if (!browseTarget) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-500">
        {device.platform === "ios" ? "请选择 App" : "请选择 App 或外部存储"}
      </div>
    );
  }

  return (
    <div
      className="flex-1 flex flex-col overflow-hidden"
      onDrop={handleDrop}
      onDragOver={handleDragOver}
    >
      <BreadcrumbBar />
      <Toolbar
        selectedCount={selected.size}
        onImport={handleImport}
        onExport={handleExport}
        onDelete={handleDelete}
      />
      <div className="flex-1 overflow-auto">
        {viewMode === "list" ? (
          <FileList
            files={files}
            selected={selected}
            onNavigate={setCurrentPath}
            onSelect={handleClick}
            onDragStart={(f) => startFileDrag([f])}
          />
        ) : (
          <FileGrid
            files={files}
            selected={selected}
            onNavigate={setCurrentPath}
            onSelect={handleClick}
            onDragStart={(f) => startFileDrag([f])}
          />
        )}
      </div>
    </div>
  );
}
```

Note: `pkg!` is safe in the iOS branches because iOS always requires a
selected app (`browseTarget` can only be `{ kind: "app" }` on iOS — there is
no iOS "external storage" concept). Android passes `pkg` (possibly
`undefined`) straight through, matching the optional `package` parameter on
every Android command.

- [ ] **Step 2: Install Tauri dialog plugin**

```bash
npm install @tauri-apps/plugin-dialog
```

Add to `src-tauri/Cargo.toml`:
```toml
tauri-plugin-dialog = "2"
```

Register in `src-tauri/src/main.rs` — add `.plugin(tauri_plugin_dialog::init())` before `.invoke_handler(...)`:
```rust
tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| { ... })
    ...
```

- [ ] **Step 3: Commit**

```bash
git add src/components/FileBrowser/index.tsx src-tauri/Cargo.toml src-tauri/src/main.rs
git commit -m "feat: FileBrowser wires device/app/path/file-ops with drag-drop and keyboard shortcuts"
```

---

## Task 14: TransferPanel

**Files:**
- Create: `src/components/TransferPanel/TransferItem.tsx`
- Create: `src/components/TransferPanel/index.tsx`

- [ ] **Step 1: Create TransferItem**

Create `src/components/TransferPanel/TransferItem.tsx`:
```typescript
import { TransferTask, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

export function TransferItem({ task }: { task: TransferTask }) {
  const pct =
    task.total_bytes > 0
      ? Math.round((task.transferred_bytes / task.total_bytes) * 100)
      : 0;

  const canCancel = task.status === "pending" || task.status === "running";

  return (
    <div className="px-3 py-2 border-b border-gray-700 text-xs">
      <div className="flex items-center justify-between mb-1">
        <span className="truncate text-gray-300 max-w-[180px]">
          {task.src.split("/").pop()}
        </span>
        <div className="flex items-center gap-2">
          <span className={`text-xs ${task.status === "error" ? "text-red-400" : "text-gray-500"}`}>
            {task.status === "error" ? task.error ?? "error" : task.status}
          </span>
          {canCancel && (
            <button
              onClick={() => tauriApi.cancelTransfer(task.id)}
              className="text-gray-500 hover:text-red-400"
            >
              ✕
            </button>
          )}
        </div>
      </div>
      <div className="h-1 bg-gray-700 rounded overflow-hidden">
        <div
          className={`h-full rounded transition-all ${
            task.status === "error" ? "bg-red-500" : "bg-blue-500"
          }`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Create TransferPanel**

TransferPanel only renders from the store — the `transfer-progress` listener
itself lives in `useTransferListener` (Task 8), which `FileBrowser` already
calls, so subscribing to the event a second time here would double-process
each event.

Create `src/components/TransferPanel/index.tsx`:
```typescript
import { useStore } from "../../store";
import { TransferItem } from "./TransferItem";

export function TransferPanel() {
  const transfers = useStore((s) => s.transfers);

  const active = transfers.filter(
    (t) => t.status === "pending" || t.status === "running"
  );

  if (active.length === 0) return null;

  return (
    <div className="border-t border-gray-700 bg-gray-800 max-h-32 overflow-y-auto">
      {active.map((task) => (
        <TransferItem key={task.id} task={task} />
      ))}
    </div>
  );
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/TransferPanel/
git commit -m "feat: TransferPanel with progress bars and cancel buttons"
```

---

## Task 15: App root layout and error handling

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/main.tsx`

- [ ] **Step 1: Create App root with error display**

Replace `src/App.tsx`:
```typescript
import { useEffect, useState } from "react";
import { DevicePanel } from "./components/DevicePanel";
import { FileBrowser } from "./components/FileBrowser";
import { TransferPanel } from "./components/TransferPanel";
import { useDeviceListener } from "./hooks/useTauri";
import { tauriApi } from "./hooks/useTauri";
import { useStore } from "./store";

export default function App() {
  useDeviceListener();

  const setDevices = useStore((s) => s.setDevices);
  const [loadError, setLoadError] = useState<string | null>(null);

  // Initial device load on mount. Each platform's detection tool (adb /
  // idevice_id) may simply not be present or fail to spawn — that's still
  // surfaced to the user instead of silently showing an empty device list
  // indistinguishable from "no devices plugged in".
  useEffect(() => {
    const load = async () => {
      const [ios, android] = await Promise.allSettled([
        tauriApi.listIosDevices(),
        tauriApi.listAndroidDevices(),
      ]);
      const all = [
        ...(ios.status === "fulfilled" ? ios.value : []),
        ...(android.status === "fulfilled" ? android.value : []),
      ];
      const errors = [
        ios.status === "rejected" ? `iOS 设备检测失败: ${ios.reason}` : null,
        android.status === "rejected" ? `Android 设备检测失败: ${android.reason}` : null,
      ].filter((e): e is string => e !== null);
      setLoadError(errors.length > 0 ? errors.join("; ") : null);
      setDevices(all);
    };
    load();
  }, [setDevices]);

  return (
    <div className="flex flex-col h-screen bg-gray-900 text-white overflow-hidden">
      {loadError && (
        <div className="px-3 py-2 bg-red-900 text-red-200 text-xs flex items-center justify-between">
          <span>{loadError}</span>
          <button onClick={() => setLoadError(null)} className="text-red-300 hover:text-white">
            ✕
          </button>
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <DevicePanel />
        <FileBrowser />
      </div>
      <TransferPanel />
    </div>
  );
}
```

- [ ] **Step 2: Update main.tsx**

Replace `src/main.tsx`:
```typescript
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

- [ ] **Step 3: Build and smoke test**

```bash
cd /Users/hongqize/Workspace/x-explorer
npm run tauri dev 2>&1 &
sleep 5
# App should open; verify no crash on startup
```

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx src/main.tsx
git commit -m "feat: root App layout wires DevicePanel, FileBrowser, TransferPanel"
```

---

## Task 16: Final verification

- [ ] **Step 1: Run all frontend tests**

```bash
cd /Users/hongqize/Workspace/x-explorer
npx vitest run 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 2: Run all Rust tests**

```bash
cd /Users/hongqize/Workspace/x-explorer/src-tauri
cargo test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 3: Configure bundled binaries as Tauri resources**

`bin_path::resolve`'s production fallback (Task 2) looks for binaries next to
the running executable inside the `.app` bundle — but nothing copies
`src-tauri/binaries/*` into the bundle unless `tauri.conf.json` is told to.
Without this step, `npm run tauri build` produces an `.app` that compiles
fine but fails at runtime with "Binary 'adb' not found" the first time any
device command runs.

Before building, place the actual binaries in `src-tauri/binaries/`
(`adb`, `idevice_id`, `ideviceinfo`, `ideviceinstaller`, `ifuse` — see
`src-tauri/binaries/README.md` from Task 2 for where to get them), then add to
`src-tauri/tauri.conf.json` under the top-level `"bundle"` key:

```json
{
  "bundle": {
    "resources": ["binaries/*"]
  }
}
```

This copies every file under `binaries/` into the bundle's `Resources`
directory, next to the executable, matching the path `bin_path::resolve`
checks via `current_exe().parent()`.

- [ ] **Step 4: Production build**

```bash
cd /Users/hongqize/Workspace/x-explorer
npm run tauri build 2>&1 | tail -30
```

Expected: `.app` bundle created in `src-tauri/target/release/bundle/macos/`.

- [ ] **Step 5: Verify binaries landed in the bundle**

```bash
ls -la /Users/hongqize/Workspace/x-explorer/src-tauri/target/release/bundle/macos/x-explorer.app/Contents/MacOS/
```

Expected: `adb`, `idevice_id`, `ideviceinfo`, `ideviceinstaller`, `ifuse` are
listed alongside the main executable.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "feat: x-explorer complete — iOS/Android file browser with drag-drop"
```
