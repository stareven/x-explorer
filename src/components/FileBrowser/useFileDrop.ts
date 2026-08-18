import { error } from "@tauri-apps/plugin-log";
import { useStore } from "../../store";
import { tauriApi, TransferFileItem } from "../../hooks/useTauri";

export function useFileDrop() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const browseTarget = useStore((s) => s.browseTarget);
  const devices = useStore((s) => s.devices);
  const currentPath = useStore((s) => s.currentPath);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const pkg = browseTarget?.kind === "app" ? browseTarget.app.bundle_id : undefined;

  // Drop files FROM Mac INTO the current device directory (external storage
  // or, if an app is selected, that app's data directory via run-as).
  async function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    if (!device || !browseTarget) return;

    const files = Array.from(e.dataTransfer.files);
    const uploadFiles = files
      .map((file) => {
        const localPath = (file as any).path; // Tauri provides file.path via drag-drop
        if (!localPath) return null;
        return {
          src: localPath,
          dst: `${currentPath.replace(/\/$/, "")}/${file.name}`,
          is_dir: false,
        };
      })
      .filter((item): item is TransferFileItem => item != null);
    if (uploadFiles.length === 0) return;

    try {
      if (device.platform === "ios") {
        await tauriApi.enqueueIosUploadBatch(device.id, pkg!, uploadFiles);
      } else {
        await tauriApi.enqueueAndroidUploadBatch(device.id, uploadFiles, pkg);
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
