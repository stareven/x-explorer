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

      try {
        if (device.platform === "ios") {
          await tauriApi.enqueueIosUpload(device.id, pkg!, localPath, remotePath);
        } else {
          await tauriApi.enqueueAndroidUpload(device.id, localPath, remotePath, pkg);
        }
      } catch (e) {
        console.error("Drop upload failed:", e);
      }
    }
  }

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }

  return { startFileDrag, handleDrop, handleDragOver };
}
