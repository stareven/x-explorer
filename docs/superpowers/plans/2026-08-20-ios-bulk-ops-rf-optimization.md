# iOS Bulk Operations `-rf` Optimization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-file subprocess spawning with single `afcclient {rm,get,put} -rf` calls, reducing a 100-file bulk delete/download from ~100 subprocess invocations to 1; add `put -rf` for directory uploads.

**Architecture:** afcclient 1.4.0's `-rf` flag triggers native AFC protocol recursive operations (`AFC_OP_REMOVE_PATH_AND_CONTENTS`, recursive `get`/`put`), handled entirely device-side in a single protocol round-trip. The Rust backend stops expanding directory operations into per-file ops in `prepare_ops`; instead, single ops reach `run_op` which calls the appropriate `-rf` command. Progress becomes indeterminate for single-subprocess operations, unchanged for multi-file batch uploads.

**Tech Stack:** Rust (Tauri backend), React + TypeScript (frontend), `afcclient` 1.4.0 (libimobiledevice)

---

## Progress Design

### 规则

| 操作类型 | 子进程数 | 进度表达 |
|---------|---------|---------|
| 删除目录（`rm -rf`） | 1 | Indeterminate：`处理中…` + 脉冲进度条 |
| 删除批量多路径（`rm -rf p1 p2 …`） | 1 | Indeterminate：`处理中…` + 脉冲进度条 |
| 下载目录（`get -rf`） | 1 | Indeterminate：`处理中…` + 脉冲进度条 |
| 上传目录（`put -rf`） | 1 | Indeterminate：`处理中…` + 脉冲进度条 |
| 上传 N 个独立文件（N × `put`） | N（并行 3） | Per-file：`12/N` + 确定性进度条（现状不变） |

### 触发条件

Indeterminate 状态在前端通过以下条件判断：

```
status === "running" && completed_files === 0 && total_files === 1
```

满足时显示"处理中…"和脉冲动画，否则显示 `completed_files / total_files` 和确定性进度条。

### 后端设置方式

对于单 subprocess 操作（`rm -rf`、`get -rf`、`put -rf`），在 `enqueue` 时设置 `total_files = 1`。完成后 `completed_files` 从 0 → 1，进度条从 Indeterminate 跳到 100% → 任务消失（TransferPanel 只显示 active 任务）。

---

## File Map

| File | Change |
|------|--------|
| `src-tauri/src/ios_client.rs` | `afc_remove` → `rm -rf`；新增 `ios_delete_batch`；`ios_download_dir` → `get -rf`；`ios_upload` 检测目录 → `put -rf`；删除 dead code |
| `src-tauri/src/transfer_queue.rs` | 新增 `IosDeleteBatch`、`IosUploadDir`；`prepare_ops` 不再展开目录；移除 `follow_up` 机制 |
| `src-tauri/src/types.rs` | 移除 `IosDeleteTarget`、`DownloadFile` |
| `src-tauri/src/lib.rs` | 注册 `enqueue_ios_upload_dir`（新增命令） |
| `src/components/TransferPanel/TransferItem.tsx` | 新增 Indeterminate 进度状态 |
| `src/components/TransferPanel/TransferItem.test.tsx` | 新增 Indeterminate 测试用例 |
| `src/hooks/useTauri.ts` | 新增 `enqueueIosUploadDir` 类型包装 |

---

### Task 1: ios_client.rs — 修复 `afc_remove` 使用 `-rf`

**Files:**
- Modify: `src-tauri/src/ios_client.rs`（`afc_remove` 函数，第 718–730 行）

- [ ] **Step 1: 更新 `afc_remove` — 加入 `-rf` flag，修正过时注释**

替换现有 `afc_remove` 函数（第 718–730 行）为：

```rust
/// afcclient's `rm -rf` triggers the AFC protocol's native recursive delete
/// (`AFC_OP_REMOVE_PATH_AND_CONTENTS`), handled device-side by the AFC daemon
/// in a single protocol round-trip — no per-file subprocess overhead.
/// The `-rf` flag bypasses the interactive confirmation prompt that plain `-r`
/// would trigger (confirmed in afcclient.c `handle_remove`: `recursive &&
/// !force` asks for confirmation; `-rf` sets both flags, skipping the prompt).
/// Failures are reported on stdout with exit code 0, so success is detected
/// by the absence of "Error:" in stdout.
fn afc_remove(device_id: &str, bundle_id: &str, remote: &str) -> Result<(), String> {
    let out = run_afcclient(device_id, bundle_id, &["rm", "-rf", remote])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || stdout.contains("Error:") {
        Err(afc_error_message(stdout.trim(), bundle_id))
    } else {
        Ok(())
    }
}
```

- [ ] **Step 2: 运行现有测试确认无回归**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 所有现有测试通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/ios_client.rs
git commit -m "fix(ios): use rm -rf in afc_remove to enable native recursive delete"
```

---

### Task 2: ios_client.rs — 新增 `ios_delete_batch` 多路径单次子进程删除

**Files:**
- Modify: `src-tauri/src/ios_client.rs`（在 `ios_delete` 函数后新增）

- [ ] **Step 1: 新增 `ios_delete_batch` 函数**

在现有 `ios_delete` 函数（约第 750 行）后新增：

```rust
/// Batch delete: multiple paths in a single afcclient subprocess call.
/// afcclient's `rm` supports multiple path arguments — `handle_remove` in
/// afcclient.c iterates over all paths, continuing past individual failures.
/// Each path is deleted with `-rf` (native recursive). This replaces the
/// previous pattern where each path was a separate subprocess spawn (~1.2s
/// overhead each).
///
/// stdout is checked for "Error:" across ALL paths; if any path failed,
/// the function returns an error. Partial success is surfaced so the user
/// can retry.
pub fn ios_delete_batch(device_id: String, bundle_id: String, remote_paths: Vec<String>) -> Result<(), String> {
    if remote_paths.is_empty() {
        return Ok(());
    }
    let safe_paths: Result<Vec<String>, String> = remote_paths
        .iter()
        .map(|p| {
            crate::file_ops::sanitize_relative_path(p)
                .map(|sp| documents_path(&sp))
                .ok_or_else(|| "路径包含非法的上级目录引用".to_string())
        })
        .collect();
    let safe_paths = safe_paths?;
    let mut args: Vec<&str> = vec!["rm", "-rf"];
    let path_refs: Vec<&str> = safe_paths.iter().map(|s| s.as_str()).collect();
    args.extend(path_refs);
    let out = run_afcclient(&device_id, &bundle_id, &args)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || stdout.contains("Error:") {
        Err(afc_error_message(stdout.trim(), &bundle_id))
    } else {
        Ok(())
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 所有测试通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/ios_client.rs
git commit -m "feat(ios): add ios_delete_batch for multi-path single-subprocess delete"
```

---

### Task 3: ios_client.rs — 简化 `ios_download_dir` 使用 `get -rf`，删除 dead code

**Files:**
- Modify: `src-tauri/src/ios_client.rs`

- [ ] **Step 1: 删除 `ios_download_recursive`，替换 `ios_download_dir` 为 `get -rf`**

删除 `ios_download_recursive` 函数（第 421–453 行），替换 `ios_download_dir`（第 675–680 行）为：

```rust
/// Downloads an entire directory tree from an iOS app container in a single
/// `afcclient get -rf` subprocess call. The `-rf` flag enables recursive
/// download with force-overwrite, handled by afcclient's `get_file` function
/// which walks the directory tree device-side and transfers each file.
///
/// Replaces the previous two-phase approach: `collect_ios_download_files`
/// (recursive `ls -l` tree walk to enumerate all leaf files) followed by
/// N parallel `afcclient get` subprocess calls.
///
/// Parent directories of `local_path` are created before the download starts.
/// Errors from afcclient are reported on stdout (not stderr) for this command.
pub fn ios_download_dir(device_id: String, bundle_id: String, remote_path: String, local_path: String) -> Result<(), String> {
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    if let Some(parent) = Path::new(&local_path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let out = run_afcclient(&device_id, &bundle_id, &["get", "-rf", &remote, &local_path])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || stdout.contains("Error:") {
        Err(afc_error_message(stdout.trim().if_empty_then(stderr.trim()), &bundle_id))
    } else {
        Ok(())
    }
}
```

Note: `get -rf` 报错在 stdout（同 `rm`），需同时检查 stdout 和 stderr。若 stdout 为空则回退到 stderr。

需要在文件顶部或 `afc_error_message` 附近加入辅助方法：

```rust
trait StrIfEmpty {
    fn if_empty_then<'a>(&'a self, other: &'a str) -> &'a str;
}
impl StrIfEmpty for str {
    fn if_empty_then<'a>(&'a self, other: &'a str) -> &'a str {
        if self.is_empty() { other } else { self }
    }
}
```

- [ ] **Step 2: 新增 `ios_upload_dir` 函数**

在 `ios_upload` 函数后新增：

```rust
/// Uploads an entire local directory tree to an iOS app container in a single
/// `afcclient put -rf` subprocess call. The `-rf` flag enables recursive
/// upload with force-overwrite, handled by afcclient's `put_file` function.
///
/// Unlike `ios_upload` (single file), this handles the case where `local_path`
/// is a directory — `afcclient put -rf` walks the local tree and transfers
/// all files in one subprocess, vs. the previous pattern of N subprocess calls
/// (one per file, parallel at MAX_JOB_PARALLELISM=3).
pub fn ios_upload_dir(device_id: String, bundle_id: String, local_path: String, remote_path: String) -> Result<(), String> {
    let safe_remote = crate::file_ops::sanitize_relative_path(&remote_path)
        .ok_or_else(|| "路径包含非法的上级目录引用".to_string())?;
    let remote = documents_path(&safe_remote);
    let out = run_afcclient(&device_id, &bundle_id, &["put", "-rf", &local_path, &remote])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || stdout.contains("Error:") {
        Err(afc_error_message(stdout.trim().if_empty_then(stderr.trim()), &bundle_id))
    } else {
        Ok(())
    }
}
```

- [ ] **Step 3: 删除 dead code**

删除以下函数（不再被任何地方调用）：
- `collect_ios_download_files_recursive`（约第 466–497 行）
- `collect_ios_download_files`（约第 617–670 行）
- `collect_ios_delete_targets_recursive`（约第 516–563 行）
- `collect_ios_delete_targets`（约第 573–615 行）

删除这些函数相关的测试（引用已删除函数的测试）：
- `test_collect_ios_download_files_recursive_collects_files_from_flat_dir`
- `test_collect_ios_download_files_recursive_recurses_into_subdirs`
- 其他引用 `collect_ios_download_files` 或 `collect_ios_delete_targets` 的测试

同步清理 `use crate::types::{...}` 中不再使用的 `DownloadFile` 和 `IosDeleteTarget` import。

- [ ] **Step 4: 运行测试，预期 transfer_queue.rs 会编译报错（在 Task 4 中修复）**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 编译错误在 transfer_queue.rs（引用了已删除的函数）——这是预期行为，Task 4 会修复。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/ios_client.rs
git commit -m "refactor(ios): use get -rf / put -rf for directory ops, remove tree-walk dead code"
```

---

### Task 4: transfer_queue.rs — 简化 `prepare_ops`，新增 `IosDeleteBatch` 和 `IosUploadDir`

**Files:**
- Modify: `src-tauri/src/transfer_queue.rs`

- [ ] **Step 1: 更新 `JobOp` enum — 新增 `IosDeleteBatch`、`IosUploadDir`，移除 `IosDeleteDir`**

将 `JobOp` enum（约第 20–34 行）改为：

```rust
#[derive(Clone)]
pub enum JobOp {
    IosDownload { device_id: String, bundle_id: String, remote_path: String, local_path: String },
    IosDownloadDir { device_id: String, bundle_id: String, remote_path: String, local_path: String },
    IosUpload { device_id: String, bundle_id: String, local_path: String, remote_path: String },
    /// Single directory upload via `put -rf`. Single subprocess, no per-file expansion.
    IosUploadDir { device_id: String, bundle_id: String, local_path: String, remote_path: String },
    IosDelete { device_id: String, bundle_id: String, remote_path: String },
    /// Multi-path batch delete. All paths passed to single `afcclient rm -rf` call.
    IosDeleteBatch { device_id: String, bundle_id: String, remote_paths: Vec<String> },
    AndroidDownload { device_id: String, remote_path: String, local_path: String, package: Option<String> },
    AndroidDownloadDir { device_id: String, remote_path: String, local_path: String, package: Option<String> },
    AndroidUpload { device_id: String, local_path: String, remote_path: String, package: Option<String> },
    AndroidDelete { device_id: String, remote_path: String, package: Option<String> },
}
```

注意移除了 `IosDeleteDir`，新增了 `IosUploadDir` 和 `IosDeleteBatch`。

- [ ] **Step 2: 删除 `follow_up` 机制，简化 `Job` struct**

将 `Job` struct 改为：

```rust
struct Job {
    task: TransferTask,
    ops: Vec<JobOp>,
}
```

删除 `enqueue_batch_with_follow_up` 方法。更新 `enqueue_batch` 直接调用 `build_batch_job`（不再需要 `build_batch_job_with_follow_up`）。

更新 `run_job` 中的 `prepare_ops` 调用——忽略 `follow_up` 返回值：

```rust
let (ops, _) = match prepare_ops(handle, &task, ops) {
    Ok(result) => result,
    Err(e) => { /* 现有错误处理 */ }
};
update_task_total_files(&mut task, ops.len());
// ...
run_ops_parallel(handle, self.tasks.clone(), task.id.clone(), ops);
```

更新 `run_ops_parallel` 签名，移除 `follow_up` 参数：

```rust
fn run_ops_parallel(
    handle: &AppHandle,
    tasks: Arc<Mutex<HashMap<String, TransferTask>>>,
    task_id: String,
    ops: Vec<JobOp>,
) {
    run_ops_wave(handle, tasks, task_id, ops);
}
```

- [ ] **Step 3: 简化 `prepare_ops` — 不再展开目录操作**

替换 `prepare_ops` 函数为：

```rust
/// Prepares the final op list for execution. Directory operations
/// (`IosDownloadDir`, `IosUploadDir`, `IosDelete`) are no longer expanded
/// into per-file ops here; they pass through to `run_op` which calls
/// `afcclient get -rf` / `put -rf` / `rm -rf` directly — single subprocess
/// per operation instead of N subprocesses.
///
/// The `follow_up` return is always empty (the two-phase delete pattern is
/// no longer needed since `rm -rf` handles recursive deletion natively).
fn prepare_ops(
    _handle: &AppHandle,
    _task: &TransferTask,
    ops: Vec<JobOp>,
) -> Result<(Vec<JobOp>, Vec<JobOp>), String> {
    Ok((ops, Vec::new()))
}
```

- [ ] **Step 4: 更新 `run_op` — 处理新 ops**

在 `run_op` 函数中，替换 `IosDeleteDir` arm 和新增 arms：

```rust
fn run_op(op: JobOp) -> Result<(), String> {
    match op {
        JobOp::IosDownload { device_id, bundle_id, remote_path, local_path } =>
            crate::ios_client::ios_download(device_id, bundle_id, remote_path, local_path),
        JobOp::IosDownloadDir { device_id, bundle_id, remote_path, local_path } =>
            crate::ios_client::ios_download_dir(device_id, bundle_id, remote_path, local_path),
        JobOp::IosUpload { device_id, bundle_id, local_path, remote_path } =>
            crate::ios_client::ios_upload(device_id, bundle_id, local_path, remote_path),
        JobOp::IosUploadDir { device_id, bundle_id, local_path, remote_path } =>
            crate::ios_client::ios_upload_dir(device_id, bundle_id, local_path, remote_path),
        JobOp::IosDelete { device_id, bundle_id, remote_path } =>
            crate::ios_client::ios_delete(device_id, bundle_id, remote_path),
        JobOp::IosDeleteBatch { device_id, bundle_id, remote_paths } =>
            crate::ios_client::ios_delete_batch(device_id, bundle_id, remote_paths),
        JobOp::AndroidDownload { device_id, remote_path, local_path, package } =>
            crate::android_client::android_download(device_id, remote_path, local_path, package),
        JobOp::AndroidDownloadDir { device_id, remote_path, local_path, package } =>
            crate::android_client::android_download_dir(device_id, remote_path, local_path, package),
        JobOp::AndroidUpload { device_id, local_path, remote_path, package } =>
            crate::android_client::android_upload(device_id, local_path, remote_path, package),
        JobOp::AndroidDelete { device_id, remote_path, package } =>
            crate::android_client::android_delete(device_id, remote_path, package),
    }
}
```

- [ ] **Step 5: 更新 `ios_device_id` — 处理 `IosDeleteBatch`**

```rust
fn ios_device_id(op: &JobOp) -> Option<&str> {
    match op {
        JobOp::IosDownload { device_id, .. }
        | JobOp::IosDownloadDir { device_id, .. }
        | JobOp::IosUpload { device_id, .. }
        | JobOp::IosUploadDir { device_id, .. }
        | JobOp::IosDelete { device_id, .. }
        | JobOp::IosDeleteBatch { device_id, .. } => Some(device_id),
        _ => None,
    }
}
```

- [ ] **Step 6: 更新 enqueue 命令**

替换 `enqueue_ios_delete_dir`（改用 `IosDelete`，因为 `rm -rf` 现在支持目录）：

```rust
#[tauri::command]
pub fn enqueue_ios_delete_dir(
    device_id: String,
    bundle_id: String,
    remote_path: String,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::IosDelete {
        device_id,
        bundle_id,
        remote_path: remote_path.clone(),
    };
    // total_files=1 → 前端显示 Indeterminate 进度
    state.enqueue("delete", &remote_path, &remote_path, op)
}
```

替换 `enqueue_ios_delete_batch`（改用 `IosDeleteBatch`）：

```rust
#[tauri::command]
pub fn enqueue_ios_delete_batch(
    device_id: String,
    bundle_id: String,
    remote_paths: Vec<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::IosDeleteBatch {
        device_id,
        bundle_id,
        remote_paths: remote_paths.clone(),
    };
    // Single op (IosDeleteBatch) → total_files=1 → Indeterminate progress
    state.enqueue_batch("delete", &format!("{} 个路径", remote_paths.len()), "", vec![op])
}
```

新增 `enqueue_ios_upload_dir` 命令：

```rust
#[tauri::command]
pub fn enqueue_ios_upload_dir(
    device_id: String,
    bundle_id: String,
    local_path: String,
    remote_path: String,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::IosUploadDir {
        device_id,
        bundle_id,
        local_path: local_path.clone(),
        remote_path: remote_path.clone(),
    };
    // Single op → total_files=1 → Indeterminate progress
    state.enqueue("upload", &local_path, &remote_path, op)
}
```

- [ ] **Step 7: 删除 dead helpers**

删除以下函数（`prepare_ops` 不再展开，这些函数不再被调用）：
- `build_ios_download_file_ops`
- `build_ios_download_ops`
- `build_download_ops`
- `build_android_download_file_ops`
- `build_android_download_ops`
- `build_batch_job_with_follow_up`

同时删除引用这些函数的测试：
- `test_build_ios_download_ops_uses_expanded_leaf_files`
- `test_build_batch_job_with_follow_up_combines_main_and_follow_up_into_total`
- `test_build_batch_job_with_follow_up_handles_empty_follow_up`

- [ ] **Step 8: 修复编译，运行测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 所有测试通过。若有编译错误，修复之（通常是未更新的 match arm 或未清理的 import）。

- [ ] **Step 9: 提交**

```bash
git add src-tauri/src/transfer_queue.rs
git commit -m "refactor(transfer): remove per-file expansion, add IosUploadDir and IosDeleteBatch ops"
```

---

### Task 5: lib.rs — 注册新命令，types.rs — 清理

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/types.rs`

- [ ] **Step 1: 在 `lib.rs` 注册 `enqueue_ios_upload_dir`**

在 `lib.rs` 的 `invoke_handler(tauri::generate_handler![...])` 中添加：

```rust
    transfer_queue::enqueue_ios_upload_dir,
```

位置放在其他 `transfer_queue::enqueue_ios_*` 命令附近（约第 51–56 行）。

- [ ] **Step 2: 从 `types.rs` 移除 `IosDeleteTarget` 和 `DownloadFile`**

删除 `IosDeleteTarget` struct 定义（不再使用）。
删除 `DownloadFile` struct 定义（不再使用，`collect_ios_download_files` 已删除）。

- [ ] **Step 3: 运行 `cargo build` 确认全量编译通过**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: 编译成功。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/src/types.rs
git commit -m "chore: register enqueue_ios_upload_dir, remove unused types"
```

---

### Task 6: 前端 — `TransferItem` Indeterminate 进度 + `useTauri` 新增上传命令

**Files:**
- Modify: `src/components/TransferPanel/TransferItem.tsx`
- Test: `src/components/TransferPanel/TransferItem.test.tsx`
- Modify: `src/hooks/useTauri.ts`

- [ ] **Step 1: 在 `useTauri.ts` 新增 `enqueueIosUploadDir`**

在 `tauriApi` 对象中添加：

```ts
  enqueueIosUploadDir: (deviceId: string, bundleId: string, localPath: string, remotePath: string) =>
    invoke<string>("enqueue_ios_upload_dir", {
      deviceId,
      bundleId,
      localPath,
      remotePath,
    }),
```

- [ ] **Step 2: 写 Indeterminate 状态的测试**

在 `src/components/TransferPanel/TransferItem.test.tsx` 中新增：

```tsx
it("renders indeterminate progress when running with no completed files", () => {
  const task: TransferTask = {
    id: "task-indeterminate",
    kind: "delete",
    src: "/large-folder",
    dst: "/large-folder",
    total_files: 1,
    completed_files: 0,
    status: "running",
  };
  const { getByText, container } = render(<TransferItem task={task} />);
  // Shows "处理中…" instead of "0/1"
  expect(getByText("处理中…")).toBeTruthy();
  // Progress bar uses animate-pulse, not a fixed percentage width
  const bar = container.querySelector(".h-full.rounded");
  expect(bar?.className).toContain("animate-pulse");
});

it("shows normal progress when completed_files > 0", () => {
  const task: TransferTask = {
    id: "task-progress",
    kind: "upload",
    src: "/local/files",
    dst: "/device/files",
    total_files: 5,
    completed_files: 3,
    status: "running",
  };
  const { getByText } = render(<TransferItem task={task} />);
  expect(getByText("3/5")).toBeTruthy();
});
```

- [ ] **Step 3: 运行测试确认失败**

Run: `npx vitest run src/components/TransferPanel/TransferItem.test.tsx`
Expected: FAIL — 当前组件总是显示 `0/1` 而非"处理中…"。

- [ ] **Step 4: 更新 `TransferItem.tsx`**

替换完整 `TransferItem.tsx` 为：

```tsx
import { TransferTask } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

export function TransferItem({ task }: { task: TransferTask }) {
  // Indeterminate: single-subprocess op (rm -rf / get -rf / put -rf) that
  // has started but not yet completed. total_files=1, completed_files=0,
  // status=running. Once done, completed_files becomes 1 and the task
  // transitions to "done" (disappears from TransferPanel).
  const isIndeterminate =
    task.status === "running" &&
    task.completed_files === 0 &&
    task.total_files === 1;

  const pct =
    task.total_files > 0 && !isIndeterminate
      ? Math.round((task.completed_files / task.total_files) * 100)
      : 0;

  const canCancel = task.status === "pending" || task.status === "running";

  return (
    <div className="px-3 py-2 border-b border-gray-700 text-xs">
      <div className="flex items-center justify-between mb-1">
        <span className="truncate text-gray-300 max-w-[180px]">
          {task.src.split("/").pop()}
        </span>
        <div className="flex items-center gap-2">
          {isIndeterminate ? (
            <span className="text-gray-400">处理中…</span>
          ) : (
            <span className="tabular-nums text-gray-400">
              {task.completed_files}/{task.total_files}
            </span>
          )}
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
        {isIndeterminate ? (
          <div
            className="h-full bg-blue-500 rounded animate-pulse"
            style={{ width: "60%" }}
          />
        ) : (
          <div
            className={`h-full rounded transition-all ${
              task.status === "error" ? "bg-red-500" : "bg-blue-500"
            }`}
            style={{ width: `${pct}%` }}
          />
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `npx vitest run src/components/TransferPanel/TransferItem.test.tsx`
Expected: PASS.

- [ ] **Step 6: 运行所有前端测试**

Run: `npx vitest run`
Expected: 全部通过。

- [ ] **Step 7: 提交**

```bash
git add src/components/TransferPanel/TransferItem.tsx src/components/TransferPanel/TransferItem.test.tsx src/hooks/useTauri.ts
git commit -m "feat(ui): indeterminate progress for single-subprocess iOS operations"
```

---

### Task 7: 全量验证

**Files:**
- 检查所有已修改文件

- [ ] **Step 1: 运行 Rust 全量构建**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: 编译成功，无 warning。

- [ ] **Step 2: 运行 `npm run build`**

Run: `npm run build`
Expected: TypeScript 类型检查通过，Vite 生产构建成功。

- [ ] **Step 3: 运行所有测试**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
npx vitest run
```

Expected: 全部通过。

- [ ] **Step 4: 最终提交（如有漏改）**

```bash
git add -A
git commit -m "chore: final cleanup after iOS bulk ops -rf optimization"
```

---

## 最终对比

| 操作 | 优化前 | 优化后 |
|------|--------|--------|
| 删除 100 文件目录 | `ls -l` × N 子目录 + `rm` × 100 ≈ 120s | `rm -rf` × 1 ≈ 3s |
| 下载 100 文件目录 | tree walk + N × `get` ≈ 120s | `get -rf` × 1 ≈ 3s + USB 传输 |
| 上传目录（N 文件） | N × `put` 并行 3 ≈ 40s | `put -rf` × 1 ≈ 3s + USB 传输 |
| 批量删除 N 路径 | N × `rm` 并行 3 | `rm -rf p1 p2 ...` × 1 |
| 上传 N 个独立文件 | N × `put` 并行 3（不变） | N × `put` 并行 3（不变，USB 限制） |
| 进度：目录操作 | 逐文件 12/100 | Indeterminate `处理中…` |
| 进度：批量文件上传 | 逐文件 12/N（不变） | 逐文件 12/N（不变） |


---

## Corrections (self-review pass 2)

**Task 3 Step 1 — use simple helper function instead of trait for error fallback:**

Remove the `StrIfEmpty` trait. Instead, add a plain function near `afc_error_message` in `ios_client.rs`:

```rust
/// Returns `primary` if non-empty, otherwise `fallback`. Used to select
/// between stdout and stderr when afcclient reports errors on one or the
/// other depending on the command (`rm`/`get`/`put` use stdout; `ideviceinfo`
/// uses stderr).
fn first_non_empty(primary: &str, fallback: &str) -> &str {
    if primary.is_empty() { fallback } else { primary }
}
```

Usage in `ios_download_dir` and `ios_upload_dir`:

```rust
let msg = first_non_empty(stdout.trim(), stderr.trim());
Err(afc_error_message(msg, &bundle_id))
```

**Task 4 Step 2 — also update `run_job` destructuring:**

Since `Job` no longer has `follow_up`, update the destructuring in `run_job`:

```rust
// Before:
let Job { mut task, ops, follow_up } = job;
// After:
let Job { mut task, ops } = job;
```

And update `prepare_ops` call site:

```rust
// Before:
let (ops, follow_up) = match prepare_ops(handle, &task, ops) { ... };
update_task_total_files(&mut task, ops.len() + follow_up.len());
// After:
let (ops, _) = match prepare_ops(handle, &task, ops) { ... };
update_task_total_files(&mut task, ops.len());
```

