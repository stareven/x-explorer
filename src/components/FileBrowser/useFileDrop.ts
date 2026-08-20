import { error } from "@tauri-apps/plugin-log";
import { useStore } from "../../store";
import { enqueueUploadBatch, enqueueUploadDir, tauriApi, TransferFileItem } from "../../hooks/useTauri";

export function useFileDrop(rememberPendingReload: (taskId: string) => void = () => {}) {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const browseTarget = useStore((s) => s.browseTarget);
  const devices = useStore((s) => s.devices);
  const currentPath = useStore((s) => s.currentPath);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const pkg = browseTarget?.kind === "app" ? browseTarget.app.bundle_id : undefined;

  // Drop files/folders FROM Mac INTO the current device directory (external
  // storage or, if an app is selected, that app's data directory via run-as).
  // A Finder drag can mix files and directories; each dropped path is probed
  // via `is_local_directory` so directories route to `enqueueUploadDir`
  // (single `put -rf` subprocess) while files route to the batch upload.
  async function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    if (!device || !browseTarget) return;

    const files = Array.from(e.dataTransfer.files);
    const items = await Promise.all(
      files.map(async (file) => {
        const localPath = (file as any).path; // Tauri provides file.path via drag-drop
        if (!localPath) return null;
        const is_dir = await tauriApi.isLocalDirectory(localPath);
        return {
          src: localPath,
          dst: `${currentPath.replace(/\/$/, "")}/${file.name}`,
          is_dir,
        };
      }),
    );
    const validItems = items.filter((item): item is TransferFileItem => item != null);
    if (validItems.length === 0) return;

    const dirs = validItems.filter((i) => i.is_dir);
    const fileItems = validItems.filter((i) => !i.is_dir);

    const taskIds: string[] = [];
    try {
      for (const dir of dirs) {
        taskIds.push(await enqueueUploadDir(device, pkg, dir.src, dir.dst));
      }
      if (fileItems.length > 0) {
        taskIds.push(await enqueueUploadBatch(device, pkg, fileItems));
      }
      // Register every task id so the transfer-progress listener refreshes the
      // directory listing when each one finishes.
      for (const id of taskIds) {
        rememberPendingReload(id);
      }
    } catch (e) {
      error(`Drop upload failed: ${e}`);
    }
  }

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }

  return { handleDrop, handleDragOver };
}
