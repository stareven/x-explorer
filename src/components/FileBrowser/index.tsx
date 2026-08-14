import { useEffect, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../../store";
import { tauriApi, useTransferListener } from "../../hooks/useTauri";
import { BreadcrumbBar } from "./BreadcrumbBar";
import { Toolbar } from "./Toolbar";
import { FileList } from "./FileList";
import { FileGrid } from "./FileGrid";
import { useSelection } from "./useSelection";
import { useDragDrop } from "./useDragDrop";

export function FileBrowser() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const browseTarget = useStore((s) => s.browseTarget);
  const devices = useStore((s) => s.devices);
  const currentPath = useStore((s) => s.currentPath);
  const files = useStore((s) => s.files);
  const setFiles = useStore((s) => s.setFiles);
  const setCurrentPath = useStore((s) => s.setCurrentPath);
  const viewMode = useStore((s) => s.viewMode);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const pkg = browseTarget?.kind === "app" ? browseTarget.app.bundle_id : undefined;
  const fileNames = files.map((f) => f.name);
  const { selected, handleClick, selectAll, clearSelection } = useSelection(fileNames);
  const { startFileDrag, handleDrop, handleDragOver } = useDragDrop();

  useTransferListener();

  async function reloadFiles(isCancelled: () => boolean = () => false) {
    if (!device || !browseTarget) return;
    try {
      const list =
        device.platform === "ios"
          ? await tauriApi.listIosFiles(device.id, pkg!, currentPath)
          : await tauriApi.listAndroidFiles(device.id, currentPath, pkg);
      if (!isCancelled()) {
        setFiles(list);
      }
    } catch (e) {
      console.error("Failed to load files:", e);
    }
  }

  useEffect(() => {
    let cancelled = false;
    reloadFiles(() => cancelled).then(() => {
      if (!cancelled) {
        clearSelection();
      }
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [device, browseTarget, currentPath]);

  const prevIosTarget = useRef<{ deviceId: string; bundleId: string } | null>(null);
  useEffect(() => {
    const prev = prevIosTarget.current;
    if (
      prev &&
      !(device?.platform === "ios" && browseTarget?.kind === "app" &&
        device.id === prev.deviceId && browseTarget.app.bundle_id === prev.bundleId)
    ) {
      tauriApi.iosUnmountContainer(prev.deviceId, prev.bundleId).catch(() => {});
    }
    prevIosTarget.current =
      device?.platform === "ios" && browseTarget?.kind === "app"
        ? { deviceId: device.id, bundleId: browseTarget.app.bundle_id }
        : null;
  }, [device, browseTarget]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === "a") {
        e.preventDefault();
        selectAll();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selectAll]);

  async function handleImport() {
    const paths = await open({ multiple: true });
    if (!paths || !device) return;
    const pathList = Array.isArray(paths) ? paths : [paths];
    for (const localPath of pathList) {
      const fileName = localPath.split("/").pop()!;
      const remotePath = `${currentPath.replace(/\/$/, "")}/${fileName}`;
      try {
        if (device.platform === "ios") {
          await tauriApi.enqueueIosUpload(device.id, pkg!, localPath, remotePath);
        } else {
          await tauriApi.enqueueAndroidUpload(device.id, localPath, remotePath, pkg);
        }
      } catch (e) {
        console.error(`Failed to enqueue upload for ${fileName}:`, e);
      }
    }
  }

  async function handleExport() {
    if (!device) return;
    const selectedFiles = files.filter((f) => selected.has(f.name));
    const destDir = await open({ directory: true });
    if (!destDir || typeof destDir !== "string") return;
    for (const file of selectedFiles) {
      const localPath = `${destDir}/${file.name}`;
      try {
        if (device.platform === "ios") {
          await tauriApi.enqueueIosDownload(device.id, pkg!, file.path, localPath);
        } else {
          await tauriApi.enqueueAndroidDownload(device.id, file.path, localPath, pkg);
        }
      } catch (e) {
        console.error(`Failed to enqueue download for ${file.name}:`, e);
      }
    }
  }

  async function handleDelete() {
    if (!device || !window.confirm(`删除选中的 ${selected.size} 个文件？`)) return;
    const selectedFiles = files.filter((f) => selected.has(f.name));
    for (const file of selectedFiles) {
      try {
        if (device.platform === "ios") {
          await tauriApi.iosDelete(device.id, pkg!, file.path);
        } else {
          await tauriApi.androidDelete(device.id, file.path, pkg);
        }
      } catch (e) {
        console.error(`Failed to delete ${file.name}:`, e);
      }
    }
    clearSelection();
    await reloadFiles();
  }

  if (!device) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-500">
        请选择设备
      </div>
    );
  }

  if (device.status !== "connected") {
    return (
      <div className="flex-1 flex items-center justify-center text-yellow-500">
        设备待信任或未授权，请在设备上确认
      </div>
    );
  }

  if (!browseTarget) {
    return (
      <div className="flex-1 flex items-center justify-center text-gray-500">
        {device.platform === "ios" ? "请选择 App" : "请选择 App 或外部存储"}
      </div>
    );
  }

  return (
    <div
      className="flex-1 flex flex-col overflow-hidden"
      onDrop={handleDrop}
      onDragOver={handleDragOver}
    >
      <BreadcrumbBar />
      <Toolbar
        selectedCount={selected.size}
        onImport={handleImport}
        onExport={handleExport}
        onDelete={handleDelete}
      />
      <div className="flex-1 overflow-auto">
        {viewMode === "list" ? (
          <FileList
            files={files}
            selected={selected}
            onNavigate={setCurrentPath}
            onSelect={handleClick}
            onDragStart={(f) => startFileDrag([f])}
          />
        ) : (
          <FileGrid
            files={files}
            selected={selected}
            onNavigate={setCurrentPath}
            onSelect={handleClick}
            onDragStart={(f) => startFileDrag([f])}
          />
        )}
      </div>
    </div>
  );
}
