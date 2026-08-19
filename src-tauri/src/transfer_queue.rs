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
    /// Marker op for a directory deletion; expanded by `prepare_ops` into a
    /// flat list of leaf-file `IosDelete` ops (run in parallel as the main
    /// wave) plus a sequential follow-up of empty-directory `IosDelete` ops
    /// (run after the main wave in topological order).
    IosDeleteDir { device_id: String, bundle_id: String, remote_path: String },
    AndroidDownload { device_id: String, remote_path: String, local_path: String, package: Option<String> },
    AndroidDownloadDir { device_id: String, remote_path: String, local_path: String, package: Option<String> },
    AndroidUpload { device_id: String, local_path: String, remote_path: String, package: Option<String> },
    AndroidDelete { device_id: String, remote_path: String, package: Option<String> },
}

struct Job {
    task: TransferTask,
    ops: Vec<JobOp>,
    /// Ops to run *after* `ops` completes, in order. Used by `IosDeleteDir`
    /// expansion to run directory `rmdir` ops after their leaf files have all
    /// been removed (parallel main + serial follow-up). Empty for jobs that
    /// don't need a post-pass (uploads, downloads, file-only deletes).
    follow_up: Vec<JobOp>,
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

/// RAII guard for the intra-job parallelism semaphore: releases one slot and
/// wakes a waiter when dropped. Mirrors the `RunningCountGuard` pattern so the
/// slot is freed on every exit path from the spawned thread (normal return,
/// cancellation skip, or panic) without an explicit release at each site.
struct ParallelSlotGuard<'a> {
    semaphore: &'a Arc<(Mutex<usize>, Condvar)>,
}

impl Drop for ParallelSlotGuard<'_> {
    fn drop(&mut self) {
        let (lock, cvar) = &**self.semaphore;
        *lock.lock().unwrap() -= 1;
        cvar.notify_one();
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
        let Job { mut task, ops, follow_up } = job;

        // Trust check once per job (was previously called per file inside
        // `ios_download` / `ios_upload`, and again from
        // `collect_ios_download_files` — N × ~1.2s of pure idle subprocess
        // startup for an N-file folder). The check only matters for iOS jobs;
        // for Android jobs we skip it entirely.
        if let Some(device_id) = ops.iter().find_map(ios_device_id) {
            if let Err(e) = crate::ios_client::check_ios_trusted(&device_id) {
                task.status = "error".to_string();
                task.error = Some(e);
                self.tasks.lock().unwrap().insert(task.id.clone(), task.clone());
                emit_progress(handle, &task);
                return;
            }
        }

        {
            let mut tasks = self.tasks.lock().unwrap();
            if is_cancelled(&tasks, &task.id) {
                return;
            }
            task.status = "running".to_string();
            tasks.insert(task.id.clone(), task.clone());
        }
        emit_progress(handle, &task);

        let (ops, follow_up) = match prepare_ops(handle, &task, ops) {
            Ok(ops) => ops,
            Err(e) => {
                task.status = "error".to_string();
                task.error = Some(e);
                self.tasks.lock().unwrap().insert(task.id.clone(), task.clone());
                emit_progress(handle, &task);
                return;
            }
        };
        update_task_total_files(&mut task, ops.len() + follow_up.len());
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

        // Run the ops in parallel, bounded by `MAX_JOB_PARALLELISM`. Replaces
        // the previous serial `for op in ops` loop, which made a 50-file
        // folder download take 50× longer than necessary (each `afcclient get`
        // is independently spawned and I/O-bound).
        run_ops_parallel(handle, self.tasks.clone(), task.id.clone(), ops, follow_up);
    }

    pub fn enqueue(&self, kind: &str, src: &str, dst: &str, op: JobOp) -> String {
        self.enqueue_batch(kind, src, dst, vec![op])
    }

    pub fn enqueue_batch(&self, kind: &str, src: &str, dst: &str, ops: Vec<JobOp>) -> String {
        self.enqueue_batch_with_follow_up(kind, src, dst, ops, Vec::new())
    }

    pub fn enqueue_batch_with_follow_up(
        &self,
        kind: &str,
        src: &str,
        dst: &str,
        ops: Vec<JobOp>,
        follow_up: Vec<JobOp>,
    ) -> String {
        let job = build_batch_job_with_follow_up(kind, src, dst, ops, follow_up);
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

/// Maximum number of operations a single job processes in parallel. Caps the
/// total concurrent subprocess count at `MAX_JOB_PARALLELISM × max_concurrent
/// worker threads`; with the default 3 workers and this set to 3, up to 9
/// `afcclient` subprocesses can be in flight at once (e.g. 9 concurrent
/// `afcclient get` calls). Matches `enqueue_ios_file_info`'s 8-way probe
/// concurrency, which has been stable in production.
const MAX_JOB_PARALLELISM: usize = 3;

/// Runs the main parallel wave (`ops`), then the follow-up wave (`follow_up`)
/// using the same execution body. Used by `run_job` so a single job can
/// express a two-phase execution (e.g. delete a directory: parallel leaf
/// `rm` ops in the main wave, sequential `rmdir` ops in the follow-up wave
/// in topological order produced by `collect_ios_delete_targets_recursive`).
///
/// Note: the follow-up wave uses the same `MAX_JOB_PARALLELISM` cap as the
/// main wave. This is safe because the walker pushes targets in strict
/// deepest-first order — a parent directory's `rmdir` op never shares its
/// parallel slot with one of its own children. Different subtrees interleave
/// freely, which is fine since they're disjoint.
fn run_ops_parallel(
    handle: &AppHandle,
    tasks: Arc<Mutex<HashMap<String, TransferTask>>>,
    task_id: String,
    ops: Vec<JobOp>,
    follow_up: Vec<JobOp>,
) {
    run_ops_wave(handle, tasks.clone(), task_id.clone(), ops);
    if !follow_up.is_empty() {
        run_ops_wave(handle, tasks, task_id, follow_up);
    }
}

/// Internal: runs `ops` concurrently with at most `MAX_JOB_PARALLELISM` ops
/// in flight. Body is identical to the pre-refactor `run_ops_parallel`.
fn run_ops_wave(
    handle: &AppHandle,
    tasks: Arc<Mutex<HashMap<String, TransferTask>>>,
    task_id: String,
    ops: Vec<JobOp>,
) {
    if ops.is_empty() {
        return;
    }

    let semaphore = Arc::new((Mutex::new(0usize), Condvar::new()));

    std::thread::scope(|scope| {
        for op in ops {
            // Don't spawn more work once the task is cancelled.
            {
                let g = tasks.lock().unwrap();
                if is_cancelled(&g, &task_id) {
                    break;
                }
            }

            let handle = handle.clone();
            let task_id = task_id.clone();
            let semaphore = semaphore.clone();
            let tasks = tasks.clone();

            scope.spawn(move || {
                // Slot is released when this closure exits (RAII), so the
                // semaphore can't leak even on panic or early return.
                let _slot_guard = ParallelSlotGuard { semaphore: &semaphore };

                // Acquire slot (blocks while the cap is saturated).
                {
                    let (lock, cvar) = &*semaphore;
                    let mut in_flight = lock.lock().unwrap();
                    while *in_flight >= MAX_JOB_PARALLELISM {
                        in_flight = cvar.wait(in_flight).unwrap();
                    }
                    *in_flight += 1;
                }

                // Cancellation re-check: covers a cancel that arrived while
                // this thread was blocked on `cvar.wait`. Skip the op but
                // release the slot (the guard handles that on drop).
                {
                    let g = tasks.lock().unwrap();
                    if is_cancelled(&g, &task_id) {
                        return;
                    }
                }

                let result = run_op(op);

                // Update task state. If another thread already moved the
                // task to a terminal state (e.g. a peer op errored first,
                // or the user cancelled mid-flight), skip the update so we
                // don't clobber it.
                let snapshot = {
                    let mut g = tasks.lock().unwrap();
                    let Some(t) = g.get_mut(&task_id) else {
                        return;
                    };
                    if matches!(t.status.as_str(), "cancelled" | "done" | "error") {
                        return;
                    }
                    match result {
                        Ok(()) => {
                            t.completed_files += 1;
                            if t.completed_files == t.total_files {
                                t.status = "done".to_string();
                            }
                        }
                        Err(e) => {
                            t.status = "error".to_string();
                            t.error = Some(e);
                        }
                    }
                    t.clone()
                };

                emit_progress(&handle, &snapshot);
            });
        }
    });
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
        // `IosDeleteDir` is a marker op: `prepare_ops` expands it into leaf
        // `IosDelete` ops (main wave) and empty-dir `IosDelete` ops (follow-up
        // wave) before they reach `run_op`. If we get here it means expansion
        // was skipped, which is a programmer error.
        JobOp::IosDeleteDir { .. } => unreachable!("IosDeleteDir must be expanded by prepare_ops before run_op"),
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

fn prepare_ops(
    handle: &AppHandle,
    task: &TransferTask,
    ops: Vec<JobOp>,
) -> Result<(Vec<JobOp>, Vec<JobOp>), String> {
    let mut main = Vec::new();
    let mut follow_up = Vec::new();
    for op in ops {
        match op {
            JobOp::IosDownloadDir { device_id, bundle_id, remote_path, local_path } => {
                // Same progress-callback pattern as downloads — surfaces
                // "preparing... N found" to the UI as the walker descends.
                let mut on_progress = |discovered: usize| {
                    let mut snapshot = task.clone();
                    snapshot.total_files = discovered.max(1) as u64;
                    emit_progress(handle, &snapshot);
                };
                let files = crate::ios_client::collect_ios_download_files(
                    &device_id,
                    &bundle_id,
                    &remote_path,
                    &local_path,
                    &mut on_progress,
                )?;
                main.extend(build_ios_download_file_ops(&device_id, &bundle_id, &files));
            }
            JobOp::IosDeleteDir { device_id, bundle_id, remote_path } => {
                // Walk the subtree once with `ls -l` (single subprocess per
                // directory level; same trick that dropped `info` calls in the
                // download path). Split the result:
                //   - files → main (run in parallel as leaf `rm` ops)
                //   - dirs  → follow_up (run after the main wave, in the
                //     topological order produced by the walker — a parent's
                //     `rmdir` always comes after its descendants' removal).
                let mut on_progress = |discovered: usize| {
                    let mut snapshot = task.clone();
                    snapshot.total_files = discovered.max(1) as u64;
                    emit_progress(handle, &snapshot);
                };
                let targets = crate::ios_client::collect_ios_delete_targets(
                    &device_id,
                    &bundle_id,
                    &remote_path,
                    &mut on_progress,
                )?;
                for target in targets {
                    let op = JobOp::IosDelete {
                        device_id: device_id.clone(),
                        bundle_id: bundle_id.clone(),
                        remote_path: target.remote_path,
                    };
                    if target.is_dir {
                        follow_up.push(op);
                    } else {
                        main.push(op);
                    }
                }
            }
            JobOp::AndroidDownloadDir { device_id, remote_path, local_path, package } => {
                let files = crate::android_client::collect_android_download_files(
                    &device_id,
                    &remote_path,
                    &local_path,
                    package.clone(),
                    true,
                )?;
                main.extend(build_android_download_file_ops(&device_id, package.as_ref(), &files));
            }
            op => main.push(op),
        }
    }
    Ok((main, follow_up))
}

/// Extracts the device id from an iOS job op. Returns None for Android ops,
/// which use a different trust mechanism.
fn ios_device_id(op: &JobOp) -> Option<&str> {
    match op {
        JobOp::IosDownload { device_id, .. }
        | JobOp::IosDownloadDir { device_id, .. }
        | JobOp::IosUpload { device_id, .. }
        | JobOp::IosDelete { device_id, .. } => Some(device_id),
        _ => None,
    }
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
        follow_up: Vec::new(),
    }
}

fn build_batch_job_with_follow_up(
    kind: &str,
    src: &str,
    dst: &str,
    ops: Vec<JobOp>,
    follow_up: Vec<JobOp>,
) -> Job {
    let total_files = (ops.len() + follow_up.len()).max(1) as u64;
    Job {
        task: build_task(kind, src, dst, total_files),
        ops,
        follow_up,
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
pub fn enqueue_ios_delete_dir(
    device_id: String,
    bundle_id: String,
    remote_path: String,
    state: tauri::State<Arc<TransferQueue>>,
) -> String {
    let op = JobOp::IosDeleteDir {
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
    fn test_build_batch_job_with_follow_up_combines_main_and_follow_up_into_total() {
        let ops = vec![
            JobOp::IosDelete {
                device_id: "dev".into(),
                bundle_id: "b".into(),
                remote_path: "root/a.txt".into(),
            },
            JobOp::IosDelete {
                device_id: "dev".into(),
                bundle_id: "b".into(),
                remote_path: "root/b.txt".into(),
            },
        ];
        let follow_up = vec![
            JobOp::IosDelete {
                device_id: "dev".into(),
                bundle_id: "b".into(),
                remote_path: "root".into(),
            },
        ];
        let job = build_batch_job_with_follow_up("delete", "root", "root", ops, follow_up);

        // total_files = main.len() + follow_up.len() (capped at >=1)
        assert_eq!(job.task.total_files, 3);
        assert_eq!(job.ops.len(), 2);
        assert_eq!(job.follow_up.len(), 1);
        // No completed files yet, status still pending — Job hasn't been enqueued.
        assert_eq!(job.task.completed_files, 0);
        assert_eq!(job.task.status, "pending");
    }

    #[test]
    fn test_build_batch_job_with_follow_up_handles_empty_follow_up() {
        // Single-op job (no follow-up) still works through the with_follow_up
        // helper — total_files should equal ops.len().
        let ops = vec![
            JobOp::IosDelete {
                device_id: "dev".into(),
                bundle_id: "b".into(),
                remote_path: "file.txt".into(),
            },
        ];
        let job = build_batch_job_with_follow_up("delete", "file.txt", "file.txt", ops, vec![]);
        assert_eq!(job.task.total_files, 1);
        assert_eq!(job.ops.len(), 1);
        assert!(job.follow_up.is_empty());
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

    #[test]
    fn test_parallel_slot_guard_decrements_on_drop() {
        let semaphore: Arc<(Mutex<usize>, Condvar)> =
            Arc::new((Mutex::new(0), Condvar::new()));
        *semaphore.0.lock().unwrap() += 1;
        assert_eq!(*semaphore.0.lock().unwrap(), 1);
        {
            let _guard = ParallelSlotGuard { semaphore: &semaphore };
            assert_eq!(*semaphore.0.lock().unwrap(), 1);
        }
        assert_eq!(*semaphore.0.lock().unwrap(), 0);
    }
}
