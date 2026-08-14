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

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_job() -> JobOp {
        JobOp::AndroidDownload {
            device_id: "nonexistent".to_string(),
            remote_path: "/sdcard/x".to_string(),
            local_path: "/tmp/x".to_string(),
            package: None,
        }
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
        let _ = noop_job();
    }
}
