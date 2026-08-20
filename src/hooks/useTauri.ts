import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { AppInfo, Device, FileEntry, TransferTask, useStore } from "../store";

export interface TransferProgress {
  task_id: string;
  kind: TransferTask["kind"];
  src: string;
  dst: string;
  completed_files: number;
  total_files: number;
  status: TransferTask["status"];
  error?: string;
}

export interface TransferFileItem {
  src: string;
  dst: string;
  is_dir: boolean;
}

export interface IosFileInfoReady {
  path: string;
  is_dir: boolean;
  size: number;
  modified?: number;
}

// Typed invoke wrappers. Functions that read/list run synchronously
// (they are fast, single round-trip shell calls). Functions that move file
// bytes (download/upload) or enqueue deletions are sent through the backend
// transfer_queue instead of awaited directly, so progress can be tracked and
// the operation can be cancelled — see transfer_queue.rs (Task 6). Android file
// operations that target an app's data directory take an optional `package`; omit
// it (or pass `undefined`) when browsing external storage.
export const tauriApi = {
  listIosDevices: () => invoke<Device[]>("list_ios_devices"),
  listAndroidDevices: () => invoke<Device[]>("list_android_devices"),
  listIosApps: (deviceId: string) => invoke<AppInfo[]>("list_ios_apps", { deviceId }),
  listAndroidApps: (deviceId: string) => invoke<AppInfo[]>("list_android_apps", { deviceId }),
  listIosFiles: (deviceId: string, bundleId: string, path: string) =>
    invoke<FileEntry[]>("list_ios_files", { deviceId, bundleId, path }),
  enqueueIosFileInfo: (deviceId: string, bundleId: string, paths: string[]) =>
    invoke<void>("enqueue_ios_file_info", { deviceId, bundleId, paths }),
  listAndroidFiles: (deviceId: string, path: string, pkg?: string) =>
    invoke<FileEntry[]>("list_android_files", { deviceId, path, package: pkg ?? null }),
  iosDeleteBatch: (deviceId: string, bundleId: string, remotePaths: string[]) =>
    invoke<string>("enqueue_ios_delete_batch", { deviceId, bundleId, remotePaths }),
  enqueueIosDeleteDir: (deviceId: string, bundleId: string, remotePath: string) =>
    invoke<string>("enqueue_ios_delete_dir", { deviceId, bundleId, remotePath }),
  androidDelete: (deviceId: string, remotePath: string, pkg?: string) =>
    invoke<string>("enqueue_android_delete", { deviceId, remotePath, package: pkg ?? null }),
  androidDeleteBatch: (deviceId: string, remotePaths: string[], pkg?: string) =>
    invoke<string>("enqueue_android_delete_batch", { deviceId, remotePaths, package: pkg ?? null }),
  // Enqueue-based transfer commands — return the new task's id immediately;
  // actual progress arrives via the "transfer-progress" event (see
  // useTransferListener below).
  enqueueIosDownload: (deviceId: string, bundleId: string, remotePath: string, localPath: string) =>
    invoke<string>("enqueue_ios_download", {
      deviceId,
      bundleId,
      remotePath,
      localPath,
    }),
  enqueueIosDownloadBatch: (deviceId: string, bundleId: string, files: TransferFileItem[]) =>
    invoke<string>("enqueue_ios_download_batch", { deviceId, bundleId, files }),
  enqueueIosUpload: (deviceId: string, bundleId: string, localPath: string, remotePath: string) =>
    invoke<string>("enqueue_ios_upload", {
      deviceId,
      bundleId,
      localPath,
      remotePath,
    }),
  enqueueIosUploadBatch: (deviceId: string, bundleId: string, files: TransferFileItem[]) =>
    invoke<string>("enqueue_ios_upload_batch", { deviceId, bundleId, files }),
  enqueueIosUploadDir: (deviceId: string, bundleId: string, localPath: string, remotePath: string) =>
    invoke<string>("enqueue_ios_upload_dir", {
      deviceId,
      bundleId,
      localPath,
      remotePath,
    }),
  isLocalDirectory: (path: string) => invoke<boolean>("is_local_directory", { path }),
  enqueueAndroidDownload: (deviceId: string, remotePath: string, localPath: string, pkg?: string) =>
    invoke<string>("enqueue_android_download", {
      deviceId,
      remotePath,
      localPath,
      package: pkg ?? null,
    }),
  enqueueAndroidDownloadBatch: (deviceId: string, files: TransferFileItem[], pkg?: string) =>
    invoke<string>("enqueue_android_download_batch", { deviceId, files, package: pkg ?? null }),
  enqueueAndroidUpload: (deviceId: string, localPath: string, remotePath: string, pkg?: string) =>
    invoke<string>("enqueue_android_upload", {
      deviceId,
      localPath,
      remotePath,
      package: pkg ?? null,
    }),
  enqueueAndroidUploadBatch: (deviceId: string, files: TransferFileItem[], pkg?: string) =>
    invoke<string>("enqueue_android_upload_batch", { deviceId, files, package: pkg ?? null }),
  cancelTransfer: (taskId: string) => invoke<boolean>("cancel_transfer", { taskId }),
};

// Platform-dispatched helpers used by FileBrowser + useFileDrop. They collapse
// the iOS-vs-Android `device.platform === "ios" ? iosApi() : androidApi()`
// duplication that would otherwise appear at every call site that has to be
// platform-aware (uploads + delete). Returns the new transfer task's id, which
// the caller hands to its `rememberPendingReload` so the post-completion
// reload listener can correlate the eventual `done` event with the right
// directory.
export function enqueueUploadBatch(
  device: Device,
  pkg: string | undefined,
  files: TransferFileItem[]
): Promise<string> {
  return device.platform === "ios"
    ? tauriApi.enqueueIosUploadBatch(device.id, pkg!, files)
    : tauriApi.enqueueAndroidUploadBatch(device.id, files, pkg);
}

export function enqueueDeleteBatch(
  device: Device,
  pkg: string | undefined,
  remotePaths: string[]
): Promise<string> {
  return device.platform === "ios"
    ? tauriApi.iosDeleteBatch(device.id, pkg!, remotePaths)
    : tauriApi.androidDeleteBatch(device.id, remotePaths, pkg);
}

export function enqueueDeleteDir(
  device: Device,
  pkg: string | undefined,
  remotePath: string
): Promise<string> {
  return device.platform === "ios"
    ? tauriApi.enqueueIosDeleteDir(device.id, pkg!, remotePath)
    : /* Android keeps the existing recursive path for now */ tauriApi.androidDelete(device.id, remotePath, pkg);
}

export function enqueueUploadDir(
  device: Device,
  pkg: string | undefined,
  localPath: string,
  remotePath: string
): Promise<string> {
  return device.platform === "ios"
    ? tauriApi.enqueueIosUploadDir(device.id, pkg!, localPath, remotePath)
    : /* Android external storage: adb push is already recursive for a directory */ tauriApi.enqueueAndroidUpload(device.id, localPath, remotePath, pkg);
}

// Hook: listen for device hotplug events and update the store's device list.
// If the currently selected device disappears (unplugged), clear the
// selection so FileBrowser doesn't keep showing a stale device's files.
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

// Hook: listen for transfer progress events and upsert into the store's
// transfers list. TransferPanel (Task 14) reads `transfers` from the store
// rather than polling. Optionally, callers can pass `onTaskComplete` to be
// notified the moment a task transitions into a terminal state (`done` or
// `error`), which FileBrowser uses to refresh its directory listing once the
// backend has finished the corresponding upload/delete — the backend runs
// jobs on a worker pool, so the listing taken right after `enqueue_*_batch`
// would still reflect the pre-operation state on the device.
export function useTransferListener(
  onTaskComplete?: (task: TransferProgress) => void
) {
  const upsertTransfer = useStore((s) => s.upsertTransfer);
  // Read `onTaskComplete` through a ref so the listener registration below
  // doesn't re-subscribe on every render. Without this, an inline-arrow
  // callback in the caller would force a Tauri listen/unlisten round-trip
  // on every render and force callers to invent their own "latest callback"
  // indirection. With it, callers can pass plain inline arrows referencing
  // fresh render-time values directly.
  const onTaskCompleteRef = useRef(onTaskComplete);
  useEffect(() => {
    onTaskCompleteRef.current = onTaskComplete;
  }, [onTaskComplete]);

  useEffect(() => {
    // Per-task previous status map. Recreated on every effect run so that
    // closed-over `prev` never survives across an unmount/remount (which
    // would let a stale "was done" leak across trees of consumers).
    const prevStatuses = new Map<string, string>();
    const unlisten = listen<TransferProgress>("transfer-progress", (event) => {
      const p = event.payload;
      const prev = prevStatuses.get(p.task_id);
      upsertTransfer({
        id: p.task_id,
        kind: p.kind,
        src: p.src,
        dst: p.dst,
        total_files: p.total_files,
        completed_files: p.completed_files,
        status: p.status,
        error: p.error,
      });
      prevStatuses.set(p.task_id, p.status);
      // Fire the completion callback when the task reaches a terminal state
      // (`done`/`error`) — and only then, even when the very first event we
      // see for a task is already terminal (e.g. a small batch whose
      // progress events arrive faster than the listener polls, or a single
      // completion that was never preceded by an intermediate progress
      // event). Repeated terminal emissions (e.g. `cancel_transfer`
      // re-publishing the snapshot) must not double-fire.
      const isTerminal = p.status === "done" || p.status === "error";
      const prevTerminal = prev === "done" || prev === "error";
      if (isTerminal && !prevTerminal) {
        onTaskCompleteRef.current?.(p);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [upsertTransfer]);
}

// Hook: listen for `ios-file-info-ready` events and patch the matching
// FileEntry's real metadata (type/size/modified) into the store as each
// probe completes — see `enqueue_ios_file_info` in ios_client.rs for why
// this arrives progressively instead of as one batch response.
export function useIosFileInfoListener() {
  const patchFileInfo = useStore((s) => s.patchFileInfo);
  useEffect(() => {
    const unlisten = listen<IosFileInfoReady>("ios-file-info-ready", (event) => {
      const { path, is_dir, size, modified } = event.payload;
      patchFileInfo(path, { is_dir, size, modified });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [patchFileInfo]);
}
