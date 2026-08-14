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
// rather than polling.
export function useTransferListener() {
  const upsertTransfer = useStore((s) => s.upsertTransfer);
  useEffect(() => {
    const unlisten = listen<TransferProgress>("transfer-progress", (event) => {
      const p = event.payload;
      const { transfers } = useStore.getState();
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
  }, [upsertTransfer]);
}
