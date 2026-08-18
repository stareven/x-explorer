import { error } from "@tauri-apps/plugin-log";
import { useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

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
        error(`Drop upload failed: ${e}`);
      }
    }
  }

  function handleDragOver(e: React.DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }

  return { handleDrop, handleDragOver };
}
