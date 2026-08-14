# iOS afcclient 迁移 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `ios_client.rs` 从 ifuse 挂载架构迁移到 afcclient 一次性子进程调用架构，去除 macFUSE/ifuse 依赖，同时把 App 列表过滤为只展示 `UIFileSharingEnabled=true` 的应用。

**Architecture:** 每次文件操作（ls/get/put/rm）都直接调用 `afcclient -u <udid> --documents <bundle_id> <cmd> <path>` 子进程，不再维护挂载点缓存（`MOUNTS` 静态 map 整体删除）。浏览路径固定相对于 `/Documents`（`--documents` 模式下的可用根目录），前端传入的相对路径统一拼接 `/Documents` 前缀后再传给 afcclient。`ideviceinstaller list -a CFBundleIdentifier -a UIFileSharingEnabled` 的 CSV 输出用于列 App 并过滤掉未开启文件共享的应用。`ios_unmount_container` 命令、`mount_ios_container`/`ifuse_args` 函数、`ios_client.rs` 顶部的 `MOUNTS` 静态变量全部删除；`main.rs` 的 invoke_handler 移除 `ios_unmount_container`；前端 `useTauri.ts`/`AppList.tsx` 中所有 `ios_unmount_container`/`iosUnmountContainer` 相关调用同步删除。

**Tech Stack:** Rust（std::process::Command）、afcclient CLI（libimobiledevice 1.4.0，已确认支持一次性非交互调用）、现有 `bin_path::resolve`/`file_ops::sanitize_relative_path`/`file_ops::join_path` 工具函数不变。

---

### Task 1: 新增 `parse_ideviceinstaller_ui_file_sharing_list` 解析函数并替换 App 列表调用

**Files:**
- Modify: `src-tauri/src/ios_client.rs:18-35`（`parse_ideviceinstaller_list` 函数，将被替换/新增）
- Modify: `src-tauri/src/ios_client.rs:117-122`（`list_ios_apps` 命令）
- Test: `src-tauri/src/ios_client.rs`（同文件内 `#[cfg(test)] mod tests`）

真机验证过的实际输出格式（`ideviceinstaller -u <udid> list -a CFBundleIdentifier -a UIFileSharingEnabled`）：

```
CFBundleIdentifier, UIFileSharingEnabled
cn.com.gf.etj, 
rn.notes.best, true
com.openai.chat, 
com.apple.Pages, true
```

第一行是表头（以 `CFBundleIdentifier` 开头），需要跳过。没有 `UIFileSharingEnabled=true` 的行要被过滤掉。这个新格式只提供 bundle id，不提供应用显示名，所以还需要额外调用 `ideviceinstaller -u <udid> -l` （旧格式，`CFBundleIdentifier - CFBundleVersion - CFBundleDisplayName`）获取显示名并按 bundle id 关联。

- [ ] **Step 1: 写失败的测试，验证新解析函数能从 UIFileSharingEnabled 输出中提取出 true 的 bundle id 集合**

在 `src-tauri/src/ios_client.rs` 的 `#[cfg(test)] mod tests` 块中添加：

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test test_parse_file_sharing_enabled_ids -- --nocapture`
Expected: FAIL，报错 `cannot find function 'parse_file_sharing_enabled_ids' in this scope`

- [ ] **Step 3: 实现 `parse_file_sharing_enabled_ids`**

在 `src-tauri/src/ios_client.rs` 中，紧跟在现有 `parse_ideviceinstaller_list` 函数之后添加：

```rust
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
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test test_parse_file_sharing_enabled_ids -- --nocapture`
Expected: PASS，3 个测试全部通过

- [ ] **Step 5: 修改 `list_ios_apps` 用两次调用组合出过滤后的 App 列表**

将 `src-tauri/src/ios_client.rs:117-122` 的：

```rust
#[tauri::command]
pub fn list_ios_apps(device_id: String) -> Result<Vec<AppInfo>, String> {
    let out = run_idevice("ideviceinstaller", &["-u", &device_id, "-l"])?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    Ok(parse_ideviceinstaller_list(&text))
}
```

替换为：

```rust
#[tauri::command]
pub fn list_ios_apps(device_id: String) -> Result<Vec<AppInfo>, String> {
    let sharing_out = run_idevice(
        "ideviceinstaller",
        &["-u", &device_id, "list", "-a", "CFBundleIdentifier", "-a", "UIFileSharingEnabled"],
    )?;
    let sharing_text = String::from_utf8_lossy(&sharing_out.stdout).to_string();
    let enabled_ids = parse_file_sharing_enabled_ids(&sharing_text);

    let list_out = run_idevice("ideviceinstaller", &["-u", &device_id, "-l"])?;
    let list_text = String::from_utf8_lossy(&list_out.stdout).to_string();
    let all_apps = parse_ideviceinstaller_list(&list_text);

    Ok(all_apps
        .into_iter()
        .filter(|app| enabled_ids.contains(&app.bundle_id))
        .collect())
}
```

- [ ] **Step 6: 运行完整测试套件确认没有破坏其它测试**

Run: `cd src-tauri && cargo test`
Expected: 之前 40 个测试全部仍然通过，加上新增的 3 个（应为 43 passed）

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ios_client.rs
git commit -m "feat: 过滤 iOS App 列表为仅 UIFileSharingEnabled=true 的应用"
```

---

### Task 2: 新增基于 afcclient 的文件列表函数，替换 `list_mounted_dir`

**Files:**
- Modify: `src-tauri/src/ios_client.rs:59-83`（删除 `list_mounted_dir`，新增 `parse_afcclient_ls`）
- Modify: `src-tauri/src/ios_client.rs:124-130`（`list_ios_files` 命令）

`afcclient ls <path>` 的输出格式是纯文件名列表，每行一个名字，没有类型/大小/时间信息（已在真机上验证：`ls /Documents` 输出仅有文件/目录名称，一行一个）。要获取 `is_dir`/`size`/`modified`，需要对每个条目额外调用 `afcclient info <path>`，其 JSON 输出包含 `st_ifmt`（`S_IFDIR`/`S_IFREG`）、`st_size`、`st_mtime`（纳秒级 Unix 时间戳）。

真机验证过的 `afcclient info <path>` 输出示例：
```json
{
  "st_size": 224,
  "st_blocks": 0,
  "st_nlink": 6,
  "st_ifmt": "S_IFDIR",
  "st_mtime": 1765271745750627872,
  "st_birthtime": 1765271744308826162
}
```

- [ ] **Step 1: 写失败的测试，验证 ls 输出解析为文件名列表**

在 `src-tauri/src/ios_client.rs` 的测试模块中添加：

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test test_parse_afcclient -- --nocapture`
Expected: FAIL，报错找不到 `parse_afcclient_ls`/`parse_afcclient_info`/`AfcFileInfo`

- [ ] **Step 3: 在 `Cargo.toml` 中确认 `serde_json` 依赖存在**

Run: `grep serde_json src-tauri/Cargo.toml`

如果没有输出，在 `src-tauri/Cargo.toml` 的 `[dependencies]` 段添加：

```toml
serde_json = "1"
```

（注：项目已用 `serde` 做 Tauri command 的序列化，`serde_json` 是同系列的标准配套库，用于手动解析 `afcclient info` 的 JSON 输出。）

- [ ] **Step 4: 实现 `parse_afcclient_ls`、`AfcFileInfo`、`parse_afcclient_info`**

删除 `src-tauri/src/ios_client.rs:59-83` 的整个 `list_mounted_dir` 函数（包括其文档注释），替换为：

```rust
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
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test test_parse_afcclient -- --nocapture`
Expected: PASS，5 个测试全部通过

- [ ] **Step 6: 新增 `run_afcclient` 辅助函数和 `documents_root`/构造 documents 路径的辅助函数**

在 `src-tauri/src/ios_client.rs` 中的 `run_idevice` 函数之后添加：

```rust
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
```

- [ ] **Step 7: 运行测试确认没有破坏其它测试（新函数暂未被调用，编译应该仍然通过）**

Run: `cd src-tauri && cargo test`
Expected: 所有测试仍然通过（`run_afcclient`/`documents_path`/`afc_error_message` 未使用会产生 warning 而非 error，此时可忽略，Task 3 会开始使用它们）

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/ios_client.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: 新增 afcclient ls/info 输出解析与子进程调用辅助函数"
```

---

### Task 3: 重写 `list_ios_files`、`ios_download`、`ios_upload`、`ios_delete` 使用 afcclient

**Files:**
- Modify: `src-tauri/src/ios_client.rs:124-167`（四个函数）

- [ ] **Step 1: 写失败的集成测试，验证 `documents_path` 与 `afc_error_message` 的边界行为（这两个是纯函数，可以直接测试；实际 afcclient 子进程调用不在单元测试范围内，遵循现有代码库模式——`run_idevice`/`android_client` 里的 adb 调用同样没有单元测试，只测试解析/路径逻辑）**

在测试模块中添加：

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test test_documents_path test_afc_error_message -- --nocapture`
Expected: FAIL（`documents_path`/`afc_error_message` 在 Task 2 已经写好，此步应直接 PASS —— 若确实已经 PASS，说明 Task 2 的 Step 7 已经覆盖，跳到 Step 3 即可）

- [ ] **Step 3: 重写 `list_ios_files`**

将 `src-tauri/src/ios_client.rs:124-130` 的：

```rust
#[tauri::command]
pub fn list_ios_files(device_id: String, bundle_id: String, path: String) -> Result<Vec<FileEntry>, String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let safe_path = crate::file_ops::sanitize_relative_path(&path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    list_mounted_dir(&mount_path, &safe_path)
}
```

替换为：

```rust
#[tauri::command]
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

    let mut result = Vec::new();
    for name in names {
        let entry_remote_path = crate::file_ops::join_path(&remote_dir, &name);
        let info_out = run_afcclient(&device_id, &bundle_id, &["info", &entry_remote_path])?;
        let info_text = String::from_utf8_lossy(&info_out.stdout).to_string();
        let info = parse_afcclient_info(&info_text).unwrap_or(AfcFileInfo {
            is_dir: false,
            size: 0,
            modified: None,
        });
        let entry_ui_path = crate::file_ops::join_path(&safe_path, &name);
        result.push(FileEntry {
            path: entry_ui_path,
            name,
            is_dir: info.is_dir,
            size: info.size,
            modified: info.modified,
        });
    }
    Ok(result)
}
```

注意：`entry_ui_path` 是相对于 App 浏览根（不含 `/Documents` 前缀）的路径，与既有的 Android/iOS `FileEntry.path` 约定一致（前端始终认为路径是"相对于当前 App 的浏览根"，不需要知道底层 afcclient 用的是 `/Documents` 子目录）。

- [ ] **Step 4: 重写 `ios_download`**

将 `src-tauri/src/ios_client.rs:132-140` 的：

```rust
/// Not a #[tauri::command] — called internally by transfer_queue only.
pub fn ios_download(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    let mount_path = mount_ios_container(&device_id, &bundle_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let src = PathBuf::from(crate::file_ops::join_path(&mount_path, &safe_remote));
    std::fs::copy(&src, &local_path).map_err(|e| e.to_string())?;
    Ok(())
}
```

替换为：

```rust
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
```

- [ ] **Step 5: 重写 `ios_upload`**

将 `src-tauri/src/ios_client.rs:142-153` 的：

```rust
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
```

替换为：

```rust
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
```

注：`afcclient put` 会在需要时自动创建远端父目录（已在真机验证 `put` 场景中确认可用，测试用例是把文件直接放在已存在的 `/Documents` 下；若未来发现子目录不存在的场景失败，需要先跑 `afcclient mkdir` 补一层，但当前迁移范围内没有验证到这个失败场景，遵循 YAGNI 不在此处添加未验证的兜底逻辑）。

- [ ] **Step 6: 重写 `ios_delete`**

将 `src-tauri/src/ios_client.rs:155-167` 的：

```rust
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
```

替换为：

```rust
#[tauri::command]
pub fn ios_delete(device_id: String, bundle_id: String, remote_path: String) -> Result<(), String> {
    check_ios_trusted(&device_id)?;
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    let out = run_afcclient(&device_id, &bundle_id, &["rm", "-rf", &remote])?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(afc_error_message(&stderr, &bundle_id))
    }
}
```

- [ ] **Step 7: 删除挂载相关代码：`MOUNTS` 静态变量、`ios_unmount_container` 命令、`mount_ios_container`、`ifuse_args` 函数**

删除 `src-tauri/src/ios_client.rs` 中以下内容（对照当前文件行号，删除后请用 Read 工具重新核对行号，因为前面的 Step 已经改变了行数）：
- 顶部的 `use std::collections::HashMap;`、`use std::sync::Mutex;`、`static MOUNTS: ...` 这三行
- `use std::path::PathBuf;`（如果删除后 `PathBuf` 已无其它用途，一并删除该 import；用 `grep -n "PathBuf" src-tauri/src/ios_client.rs` 确认）
- `ios_unmount_container` 整个函数（`#[tauri::command] pub fn ios_unmount_container...`）
- `mount_ios_container` 整个函数
- `ifuse_args` 整个函数
- 测试模块中的 `test_ifuse_args_builds_expected_argv`

Run: `grep -n "MOUNTS\|mount_ios_container\|ifuse_args\|ios_unmount_container\|PathBuf" src-tauri/src/ios_client.rs`
Expected: 无匹配（全部清除干净）

- [ ] **Step 8: 从 `main.rs` 的 invoke_handler 中移除 `ios_client::ios_unmount_container`**

在 `src-tauri/src/lib.rs` 中，将：

```rust
        .invoke_handler(tauri::generate_handler![
            ios_client::list_ios_devices,
            ios_client::list_ios_apps,
            ios_client::list_ios_files,
            ios_client::ios_delete,
            ios_client::ios_unmount_container,
            android_client::list_android_devices,
```

改为：

```rust
        .invoke_handler(tauri::generate_handler![
            ios_client::list_ios_devices,
            ios_client::list_ios_apps,
            ios_client::list_ios_files,
            ios_client::ios_delete,
            android_client::list_android_devices,
```

- [ ] **Step 9: 编译并运行完整测试套件**

Run: `cd src-tauri && cargo build 2>&1 | tail -30`
Expected: 编译成功，无 error（可能有关于未使用 import 的 warning，需要按 warning 提示清理）

Run: `cd src-tauri && cargo test`
Expected: 全部测试通过（原 40 个测试中的 `test_ifuse_args_builds_expected_argv` 已删除，加上 Task 1/2/3 新增的测试，预期约 51 个测试全部 PASS，0 FAIL）

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/ios_client.rs src-tauri/src/lib.rs
git commit -m "feat: iOS 文件操作迁移到 afcclient 一次性调用，移除 ifuse 挂载架构"
```

---

### Task 4: 更新前端，移除 `ios_unmount_container` 相关调用

**Files:**
- Modify: `src/hooks/useTauri.ts:33-34`（`iosUnmountContainer` 定义）
- Modify: `src/hooks/useTauri.ts:70-99`（`useDeviceListener` 中调用它的逻辑）

- [ ] **Step 1: 检查现有前端测试是否覆盖了 `useDeviceListener`/`iosUnmountContainer`**

Run: `grep -rn "iosUnmountContainer\|useDeviceListener" src/ --include=*.test.ts --include=*.test.tsx`

如果有匹配的测试文件，先读取它们（用 Read 工具打开每个匹配文件），了解需要同步修改的断言。

- [ ] **Step 2: 从 `src/hooks/useTauri.ts` 中删除 `iosUnmountContainer` 定义**

删除：

```ts
  iosUnmountContainer: (deviceId: string, bundleId: string) =>
    invoke<void>("ios_unmount_container", { device_id: deviceId, bundle_id: bundleId }),
```

- [ ] **Step 3: 修改 `useDeviceListener`，移除断开时调用 `iosUnmountContainer` 的逻辑**

将：

```ts
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
```

替换为：

```ts
export function useDeviceListener() {
  const setDevices = useStore((s) => s.setDevices);
  useEffect(() => {
    const unlisten = listen<Device[]>("devices-changed", (event) => {
      const nextDevices = event.payload;
      const { selectedDeviceId, setSelectedDeviceId } = useStore.getState();
      const stillPresent = new Set(nextDevices.map((d) => d.id));
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
```

同时更新该函数上方的文档注释（原注释提到 "unmount any iOS container belonging to a device that just disappeared... Without this, ifuse mounts under $TMPDIR/x-explorer/<device_id>/ leak until the app restarts."），改为：

```ts
// Hook: listen for device hotplug events and update the store's device list.
// If the currently selected device disappears (unplugged), clear the
// selection so FileBrowser doesn't keep showing a stale device's files.
```

- [ ] **Step 4: 运行前端测试套件**

Run: `npm test`
Expected: 全部测试通过；如果有测试断言了 `iosUnmountContainer` 被调用，需要按 Step 1 找到的测试文件更新/删除对应断言（无法在此提前列出具体断言内容，需要 Step 1 的实际读取结果决定怎么改——若发现相关测试，参照该测试文件里其它测试的写法调整，保持同样的 mock/assert 风格）

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useTauri.ts
git commit -m "feat: 移除前端 ios_unmount_container 调用（afcclient 无挂载状态需要清理）"
```

（若 Step 4 额外修改了测试文件，一并加入本次 commit。）

---

### Task 5: 更新 `binaries/README.md`，将 `ifuse` 替换为 `afcclient`

**Files:**
- Modify: `src-tauri/binaries/README.md`

- [ ] **Step 1: 更新 README 内容**

将 `src-tauri/binaries/README.md` 整个文件内容替换为：

```markdown
# Bundled Binaries

Place platform binaries here before building:

- adb                (Android Debug Bridge, from Android SDK platform-tools)
- idevice_id         (from libimobiledevice)
- ideviceinfo        (from libimobiledevice — used for trust-state detection)
- ideviceinstaller
- afcclient          (from libimobiledevice — used for all iOS app file access via `--documents`)

Download libimobiledevice tools: brew install libimobiledevice
Download adb: brew install android-platform-tools

Note: `ifuse`/macFUSE is no longer required. iOS file access now goes
through `afcclient --documents <bundle_id>`, a one-shot subprocess call
that doesn't need a kernel extension or user-approved mount. This only
works for apps with `UIFileSharingEnabled=true` in their Info.plist — this
is an iOS platform restriction on the house_arrest service, not specific
to afcclient (ifuse had the exact same restriction).
```

- [ ] **Step 2: 复制真实的 `afcclient` 二进制到本地 binaries 目录，供后续本地开发/测试使用**

Run: `cp $(which afcclient) src-tauri/binaries/afcclient`
Expected: 命令成功，`src-tauri/binaries/afcclient` 文件存在

Run: `ls -la src-tauri/binaries/`
Expected: 能看到 `afcclient` 文件

- [ ] **Step 3: 确认 `.gitignore` 是否已经忽略 `binaries/` 下的实际二进制（避免误提交平台特定的二进制文件到仓库）**

Run: `git check-ignore -q src-tauri/binaries/afcclient && echo IGNORED || echo NOT_IGNORED`

如果输出 `NOT_IGNORED`，说明其它二进制（如现有的 `adb`/`idevice_id` 等）当前是怎么处理的需要先确认：

Run: `git status src-tauri/binaries/`

若 `git status` 显示这些二进制文件均为 untracked（说明已有 `.gitignore` 规则或它们从未被添加过），保持一致，不要把 `afcclient` add 进 git。若显示为 tracked，则按现有约定处理（此计划不引入新的 git 追踪策略）。

- [ ] **Step 4: Commit（只提交 README，不提交二进制文件本身）**

```bash
git add src-tauri/binaries/README.md
git commit -m "docs: 更新 binaries README，用 afcclient 替换 ifuse"
```

---

### Task 6: 端到端真机验证

**Files:**
- 无文件改动，仅验证

- [ ] **Step 1: 启动开发模式**

Run: `npm run tauri dev`
Expected: 应用窗口正常打开，无编译错误

- [ ] **Step 2: 连接一台已信任的真机 iPhone，在 UI 中验证：**
  - 设备出现在设备列表，状态为「已连接」
  - App 列表只显示曾经在真机验证过 `UIFileSharingEnabled=true` 的应用（如 AsTools pro / 百度地图开发版等，具体取决于测试机上安装的应用）
  - 点击一个 App，能看到 `/Documents` 下的文件列表，包含正确的文件名、是否为目录、大小
  - 导出（下载）一个文件到 Mac 本地，验证下载后本地文件内容与设备上一致
  - 导入（上传）一个本地文件到该 App，验证上传后在文件列表中出现
  - 删除刚上传的文件，验证文件列表中消失

- [ ] **Step 3: 记录验证结果**

如果任何一步失败，停止后续任务，诊断根因（参考本次会话中使用 `afcclient -d`（debug 模式）抓取协议层日志的方法），不要想当然地"猜测性修补"。

---

## 后续（Out of Scope，不在本计划范围内）

以下内容已在设计讨论中提及，但确认不属于当前迁移范围，特此记录以免遗漏：
- Developer Mode 检测/提示：本次真机测试确认当前问题与 Developer Mode 无关（根因是 `--documents` 模式下 `/` 根路径本身不可 `ls`，而非设备未开启开发者模式），因此设计文档中曾讨论过的"检测 Developer Mode 并提示用户开启"的错误处理逻辑不再需要，未纳入本计划的任何 Task。
- `--container` 完整容器访问：已确认对常规 App 不可用，不会再尝试实现。
