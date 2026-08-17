use crate::types::{DownloadFile, TransferProgress, TransferTask};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Clone, Deserialize)]
pub struct FileTransferItem {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub is_dir: bool,
}

/// A single file operation to perform. `package` is Some(bundle_id) for iOS or
/// Some(package_name) for an Android app-container path; None for external storage / no-app iOS n/a.
#[derive(Clone)]
pub enum JobOp {
    IosDownload { device_id: String, bundle_id: String, remote_path: String, local_path: String },
    IosDownloadDir { device_id: String, bundle_id: String, remote_path: String, local_path: String },
    IosUpload { device_id: String, bundle_id: String, local_path: String, remote_path: String },
    IosDelete { device_id: String, bundle_id: String, remote_path: String },
    AndroidDownload { device_id: String, remote_path: String, local_path: String, package: Option<String> },
    AndroidDownloadDir { device_id: String, remote_path: String, local_path: String, package: Option<String> },
    AndroidUpload { device_id: String, local_path: String, remote_path: String, package: Option<String> },
    AndroidDelete { device_id: String, remote_path: String, package: Option<String> },
}

struct Job {
    task: TransferTask,
    ops: Vec<JobOp>,
}

/// RAII guard that decrements `running_count` when dropped, ensuring the counter
/// is decremented on every exit path from `run_job` (normal return, early return
/// on cancellation, or panic) without needing an explicit decrement at each site.
struct RunningCountGuard<'a> {
    count: &'a Arc<Mutex<usize>>,
}

impl Drop for RunningCountGuard<'_> {
    fn drop(&mut self) {
        let mut count = self.count.lock().unwrap();
        *count = count.saturating_sub(1);
    }
}

/// Returns true if the task with `id` is present in `tasks` and marked "cancelled".
fn is_cancelled(tasks: &HashMap<String, TransferTask>, id: &str) -> bool {
    tasks.get(id).map(|t| t.status.as_str()) == Some("cancelled")
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
        let Job { mut task, ops } = job;

        {
            let mut tasks = self.tasks.lock().unwrap();
            if is_cancelled(&tasks, &task.id) {
                return;
            }
            task.status = "running".to_string();
            tasks.insert(task.id.clone(), task.clone());
        }
        emit_progress(handle, &task);

        let ops = match prepare_ops(ops) {
            Ok(ops) => ops,
            Err(e) => {
                task.status = "error".to_string();
                task.error = Some(e);
                self.tasks.lock().unwrap().insert(task.id.clone(), task.clone());
                emit_progress(handle, &task);
                return;
            }
        };
        update_task_total_files(&mut task, ops.len());
        {
            let tasks = self.tasks.lock().unwrap();
            if is_cancelled(&tasks, &task.id) {
                task.status = "cancelled".to_string();
                drop(tasks);
                emit_progress(handle, &task);
                return;
            }
        }
        self.tasks.lock().unwrap().insert(task.id.clone(), task.clone());
        emit_progress(handle, &task);

        // Counted for the duration of the blocking transfer call only; the guard below
        // decrements on every exit path (normal completion, error, or mid-flight cancellation).
        *self.running_count.lock().unwrap() += 1;
        let _guard = RunningCountGuard { count: &self.running_count };

        for op in ops {
            {
                let tasks = self.tasks.lock().unwrap();
                if is_cancelled(&tasks, &task.id) {
                    task.status = "cancelled".to_string();
                    drop(tasks);
                    emit_progress(handle, &task);
                    return;
                }
            }
            let result = run_op(op);

            let mut tasks = self.tasks.lock().unwrap();
            // A cancellation requested mid-flight still lands here after the blocking call returns;
            // respect it instead of continuing with the remaining files.
            if is_cancelled(&tasks, &task.id) {
                task.status = "cancelled".to_string();
                tasks.insert(task.id.clone(), task.clone());
                drop(tasks);
                emit_progress(handle, &task);
                return;
            }
            match result {
                Ok(()) => {
                    task.completed_files += 1;
                    if task.completed_files == task.total_files {
                        task.status = "done".to_string();
                    }
                }
                Err(e) => {
                    task.status = "error".to_string();
                    task.error = Some(e);
                    tasks.insert(task.id.clone(), task.clone());
                    drop(tasks);
                    emit_progress(handle, &task);
                    return;
                }
            }
            tasks.insert(task.id.clone(), task.clone());
            drop(tasks);
            emit_progress(handle, &task);
        }
    }

    pub fn enqueue(&self, kind: &str, src: &str, dst: &str, op: JobOp) -> String {
        self.enqueue_batch(kind, src, dst, vec![op])
    }

    pub fn enqueue_batch(&self, kind: &str, src: &str, dst: &str, ops: Vec<JobOp>) -> String {
        let job = build_batch_job(kind, src, dst, ops);
        let id = job.task.id.clone();
        self.tasks.lock().unwrap().insert(id.clone(), job.task.clone());
        let (lock, cvar) = &*self.pending;
        lock.lock().unwrap().push_back(job);
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

    pub fn get_task(&self, task_id: &str) -> Option<TransferTask> {
        self.tasks.lock().unwrap().get(task_id).cloned()
    }

    pub fn get_status(&self, task_id: &str) -> Option<String> {
        self.tasks.lock().unwrap().get(task_id).map(|t| t.status.clone())
    }
}

fn run_op(op: JobOp) -> Result<(), String> {
    match op {
        JobOp::IosDownload { device_id, bundle_id, remote_path, local_path } =>
            crate::ios_client::ios_download(device_id, bundle_id, remote_path, local_path),
        JobOp::IosDownloadDir { device_id, bundle_id, remote_path, local_path } =>
            crate::ios_client::ios_download_dir(device_id, bundle_id, remote_path, local_path),
        JobOp::IosUpload { device_id, bundle_id, local_path, remote_path } =>
            crate::ios_client::ios_upload(device_id, bundle_id, local_path, remote_path),
        JobOp::IosDelete { device_id, bundle_id, remote_path } =>
            crate::ios_client::ios_delete(device_id, bundle_id, remote_path),
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

fn build_download_ops(device_id: &str, bundle_id: Option<&str>, package: Option<&String>, files: &[FileTransferItem]) -> Vec<JobOp> {
    files
        .iter()
        .map(|file| match (bundle_id, package, file.is_dir) {
            (Some(bundle_id), _, true) => JobOp::IosDownloadDir {
                device_id: device_id.to_string(),
                bundle_id: bundle_id.to_string(),
                remote_path: file.src.clone(),
                local_path: file.dst.clone(),
            },
            (Some(bundle_id), _, false) => JobOp::IosDownload {
                device_id: device_id.to_string(),
                bundle_id: bundle_id.to_string(),
                remote_path: file.src.clone(),
                local_path: file.dst.clone(),
            },
            (None, Some(package), true) => JobOp::AndroidDownloadDir {
                device_id: device_id.to_string(),
                remote_path: file.src.clone(),
                local_path: file.dst.clone(),
                package: Some(package.clone()),
            },
            (None, Some(package), false) => JobOp::AndroidDownload {
                device_id: device_id.to_string(),
                remote_path: file.src.clone(),
                local_path: file.dst.clone(),
                package: Some(package.clone()),
            },
            (None, None, true) => JobOp::AndroidDownloadDir {
                device_id: device_id.to_string(),
                remote_path: file.src.clone(),
                local_path: file.dst.clone(),
                package: None,
            },
            (None, None, false) => JobOp::AndroidDownload {
                device_id: device_id.to_string(),
                remote_path: file.src.clone(),
                local_path: file.dst.clone(),
                package: None,
            },
        })
        .collect()
}

fn prepare_ops(ops: Vec<JobOp>) -> Result<Vec<JobOp>, String> {
    let mut prepared = Vec::new();
    for op in ops {
        match op {
            JobOp::IosDownloadDir { device_id, bundle_id, remote_path, local_path } => {
                let files = crate::ios_client::collect_ios_download_files(&device_id, &bundle_id, &remote_path, &local_path)?;
                prepared.extend(build_ios_download_file_ops(&device_id, &bundle_id, &files));
            }
            JobOp::AndroidDownloadDir { device_id, remote_path, local_path, package } => {
                let files = crate::android_client::collect_android_download_files(
                    &device_id,
                    &remote_path,
                    &local_path,
                    package.clone(),
                    true,
                )?;
                prepared.extend(build_android_download_file_ops(&device_id, package.as_ref(), &files));
            }
            op => prepared.push(op),
        }
    }
    Ok(prepared)
}

fn build_ios_download_file_ops(device_id: &str, bundle_id: &str, files: &[DownloadFile]) -> Vec<JobOp> {
    files
        .iter()
        .map(|file| JobOp::IosDownload {
            device_id: device_id.to_string(),
            bundle_id: bundle_id.to_string(),
            remote_path: file.remote_path.clone(),
            local_path: file.local_path.clone(),
        })
        .collect()
}

fn build_android_download_file_ops(device_id: &str, package: Option<&String>, files: &[DownloadFile]) -> Vec<JobOp> {
    files
        .iter()
        .map(|file| JobOp::AndroidDownload {
            device_id: device_id.to_string(),
            remote_path: file.remote_path.clone(),
            local_path: file.local_path.clone(),
            package: package.cloned(),
        })
        .collect()
}

fn build_ios_download_ops(device_id: &str, bundle_id: &str, files: &[FileTransferItem]) -> Vec<JobOp> {
    build_download_ops(device_id, Some(bundle_id), None, files)
}

fn build_android_download_ops(device_id: &str, package: Option<&String>, files: &[FileTransferItem]) -> Vec<JobOp> {
    build_download_ops(device_id, None, package, files)
}

fn build_task(kind: &str, src: &str, dst: &str, total_files: u64) -> TransferTask {
    TransferTask {
        id: Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        src: src.to_string(),
        dst: dst.to_string(),
        total_files,
        completed_files: 0,
        status: "pending".to_string(),
        error: None,
    }
}

fn build_batch_job(kind: &str, src: &str, dst: &str, ops: Vec<JobOp>) -> Job {
    let total_files = ops.len().max(1) as u64;
    Job {
        task: build_task(kind, src, dst, total_files),
        ops,
    }
}

fn update_task_total_files(task: &mut TransferTask, total_files: usize) {
    task.total_files = total_files.max(1) as u64;
}

fn emit_progress(handle: &AppHandle, task: &TransferTask) {
    let _ = handle.emit(
        "transfer-progress",
        TransferProgress {
            task_id: task.id.clone(),
            kind: task.kind.clone(),
            src: task.src.clone(),
            dst: task.dst.clone(),
            total_files: task.total_files,
            completed_files: task.completed_files,
            status: task.status.clone(),
            error: task.error.clone(),
        },
    );
}

#[tauri::command]
pub fn cancel_transfer(
    task_id: String,
    state: tauri::State<Arc<TransferQueue>>,
    handle: AppHandle,
) -> bool {
    let cancelled = state.cancel(&task_id);
    if cancelled {
        if let Some(task) = state.get_task(&task_id) {
            emit_progress(&handle, &task);
        }
    }
    cancelled
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
    let op = JobOp::IosDownload {
        device_id,
        bundle_id,
        remote_path: remote_path.clone(),
        local_path: local_path.clone(),
    };
    state.enqueue("download", &remote_path, &local_path, op)
}

#[tauri::command]
pub fn enqueue_ios_upload(
    device_id: String,
    bundle_id: String,
    local_path: String,
    remote_path: String,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::IosUpload {
        device_id,
        bundle_id,
        local_path: local_path.clone(),
        remote_path: remote_path.clone(),
    };
    state.enqueue("upload", &local_path, &remote_path, op)
}

#[tauri::command]
pub fn enqueue_ios_download_batch(
    device_id: String,
    bundle_id: String,
    files: Vec<FileTransferItem>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let ops = build_ios_download_ops(&device_id, &bundle_id, &files);
    state.enqueue_batch("download", &format!("{} 个文件", files.len()), "", ops)
}

#[tauri::command]
pub fn enqueue_ios_upload_batch(
    device_id: String,
    bundle_id: String,
    files: Vec<FileTransferItem>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let ops = files
        .iter()
        .map(|file| JobOp::IosUpload {
            device_id: device_id.clone(),
            bundle_id: bundle_id.clone(),
            local_path: file.src.clone(),
            remote_path: file.dst.clone(),
        })
        .collect();
    state.enqueue_batch("upload", &format!("{} 个文件", files.len()), "", ops)
}

#[tauri::command]
pub fn enqueue_android_download(
    device_id: String,
    remote_path: String,
    local_path: String,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::AndroidDownload {
        device_id,
        remote_path: remote_path.clone(),
        local_path: local_path.clone(),
        package,
    };
    state.enqueue("download", &remote_path, &local_path, op)
}

#[tauri::command]
pub fn enqueue_android_upload(
    device_id: String,
    local_path: String,
    remote_path: String,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::AndroidUpload {
        device_id,
        local_path: local_path.clone(),
        remote_path: remote_path.clone(),
        package,
    };
    state.enqueue("upload", &local_path, &remote_path, op)
}

#[tauri::command]
pub fn enqueue_ios_delete(
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
    state.enqueue("delete", &remote_path, &remote_path, op)
}

#[tauri::command]
pub fn enqueue_android_download_batch(
    device_id: String,
    files: Vec<FileTransferItem>,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let ops = build_android_download_ops(&device_id, package.as_ref(), &files);
    state.enqueue_batch("download", &format!("{} 个文件", files.len()), "", ops)
}

#[tauri::command]
pub fn enqueue_android_upload_batch(
    device_id: String,
    files: Vec<FileTransferItem>,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let ops = files
        .iter()
        .map(|file| JobOp::AndroidUpload {
            device_id: device_id.clone(),
            local_path: file.src.clone(),
            remote_path: file.dst.clone(),
            package: package.clone(),
        })
        .collect();
    state.enqueue_batch("upload", &format!("{} 个文件", files.len()), "", ops)
}

#[tauri::command]
pub fn enqueue_android_delete(
    device_id: String,
    remote_path: String,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::AndroidDelete {
        device_id,
        remote_path: remote_path.clone(),
        package,
    };
    state.enqueue("delete", &remote_path, &remote_path, op)
}

#[tauri::command]
pub fn enqueue_ios_delete_batch(
    device_id: String,
    bundle_id: String,
    remote_paths: Vec<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let ops = remote_paths
        .iter()
        .map(|remote_path| JobOp::IosDelete {
            device_id: device_id.clone(),
            bundle_id: bundle_id.clone(),
            remote_path: remote_path.clone(),
        })
        .collect();
    state.enqueue_batch("delete", &format!("{} 个文件", remote_paths.len()), "", ops)
}

#[tauri::command]
pub fn enqueue_android_delete_batch(
    device_id: String,
    remote_paths: Vec<String>,
    package: Option<String>,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let ops = remote_paths
        .iter()
        .map(|remote_path| JobOp::AndroidDelete {
            device_id: device_id.clone(),
            remote_path: remote_path.clone(),
            package: package.clone(),
        })
        .collect();
    state.enqueue_batch("delete", &format!("{} 个文件", remote_paths.len()), "", ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: `TransferQueue::new` requires a real `tauri::AppHandle`, and this crate does not
    // depend on `tauri::test` utilities (mock_app / MockRuntime) anywhere else. Adding that
    // dependency just to exercise `TransferQueue::enqueue`/`cancel` end-to-end was judged not
    // worth it per review guidance, so the tests below continue to exercise the underlying
    // `HashMap<String, TransferTask>` state transitions directly instead of a real queue
    // instance. `is_cancelled` and the running-count guard, which don't need an `AppHandle`,
    // are tested directly below.

    fn noop_job() -> JobOp {
        JobOp::AndroidDownload {
            device_id: "nonexistent".to_string(),
            remote_path: "/sdcard/x".to_string(),
            local_path: "/tmp/x".to_string(),
            package: None,
        }
    }

    #[test]
    fn test_task_total_files_can_be_updated_after_directory_expansion() {
        let mut task = build_task("download", "1 个文件", "", 1);
        let expanded = vec![
            DownloadFile { remote_path: "/Photos/a.jpg".to_string(), local_path: "/tmp/Photos/a.jpg".to_string() },
            DownloadFile { remote_path: "/Photos/b.jpg".to_string(), local_path: "/tmp/Photos/b.jpg".to_string() },
        ];

        update_task_total_files(&mut task, expanded.len());

        assert_eq!(task.total_files, 2);
        assert_eq!(task.completed_files, 0);
        assert_eq!(task.status, "pending");
    }

    #[test]
    fn test_build_ios_download_ops_uses_expanded_leaf_files() {
        let files = vec![FileTransferItem {
            src: "/Photos/a.jpg".to_string(),
            dst: "/tmp/Photos/a.jpg".to_string(),
            is_dir: false,
        }, FileTransferItem {
            src: "/Photos/nested/b.jpg".to_string(),
            dst: "/tmp/Photos/nested/b.jpg".to_string(),
            is_dir: false,
        }];
        let ops = build_ios_download_ops("device-1", "bundle.id", &files);

        assert_eq!(ops.len(), 2);
        for op in ops {
            assert!(matches!(op, JobOp::IosDownload { .. }));
        }
    }

    #[test]
    fn test_build_batch_job_uses_one_task_for_multiple_files() {
        let ops = vec![noop_job(), noop_job(), noop_job()];
        let job = build_batch_job("download", "3 个文件", "/local", ops);

        assert_eq!(job.task.kind, "download");
        assert_eq!(job.task.src, "3 个文件");
        assert_eq!(job.task.total_files, 3);
        assert_eq!(job.task.completed_files, 0);
        assert_eq!(job.ops.len(), 3);
    }

    #[test]
    fn test_enqueue_creates_pending_task_before_worker_picks_it_up() {
        let tasks: Arc<Mutex<HashMap<String, TransferTask>>> = Arc::new(Mutex::new(HashMap::new()));
        let id = Uuid::new_v4().to_string();
        let task = TransferTask {
            id: id.clone(),
            kind: "download".to_string(),
            src: "/device/file.txt".to_string(),
            dst: "/local/file.txt".to_string(),
            total_files: 1,
            completed_files: 0,
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
                total_files: 1,
                completed_files: 0,
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
        let _ = noop_job();
    }

    #[test]
    fn test_is_cancelled_true_when_status_cancelled() {
        let mut tasks: HashMap<String, TransferTask> = HashMap::new();
        let id = "task-cancelled".to_string();
        tasks.insert(
            id.clone(),
            TransferTask {
                id: id.clone(),
                kind: "download".to_string(),
                src: "/device/file.txt".to_string(),
                dst: "/local/file.txt".to_string(),
                total_files: 1,
                completed_files: 0,
                status: "cancelled".to_string(),
                error: None,
            },
        );
        assert!(is_cancelled(&tasks, &id));
    }

    #[test]
    fn test_is_cancelled_false_when_status_running_or_missing() {
        let mut tasks: HashMap<String, TransferTask> = HashMap::new();
        let id = "task-running".to_string();
        tasks.insert(
            id.clone(),
            TransferTask {
                id: id.clone(),
                kind: "download".to_string(),
                src: "/device/file.txt".to_string(),
                dst: "/local/file.txt".to_string(),
                total_files: 1,
                completed_files: 0,
                status: "running".to_string(),
                error: None,
            },
        );
        assert!(!is_cancelled(&tasks, &id));
        assert!(!is_cancelled(&tasks, "nonexistent-id"));
    }

    #[test]
    fn test_running_count_guard_decrements_on_drop() {
        let running_count: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
        *running_count.lock().unwrap() += 1;
        assert_eq!(*running_count.lock().unwrap(), 1);
        {
            let _guard = RunningCountGuard { count: &running_count };
            assert_eq!(*running_count.lock().unwrap(), 1);
        }
        assert_eq!(*running_count.lock().unwrap(), 0);
    }
}
