import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileEntry, parentPath, useStore } from "../../store";
import { tauriApi, useIosFileInfoListener, useTransferListener } from "../../hooks/useTauri";
import { BreadcrumbBar } from "./BreadcrumbBar";
import { Toolbar } from "./Toolbar";
import { FileList } from "./FileList";
import { FileGrid } from "./FileGrid";
import { useSelection } from "./useSelection";
import { useFileDrop } from "./useFileDrop";

// Stale-while-revalidate directory listing cache: revisiting a previously
// browsed path renders the cached entries instantly while a fresh listing is
// fetched in the background to replace them. Keyed by device + browse
// target + path so switching device/app never serves wrong entries.
const listCache = new Map<string, FileEntry[]>();

function cacheKey(platform: string, deviceId: string, pkg: string | undefined, path: string) {
  return `${platform}:${deviceId}:${pkg ?? "-"}:${path}`;
}

export function FileBrowser() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const browseTarget = useStore((s) => s.browseTarget);
  const devices = useStore((s) => s.devices);
  const currentPath = useStore((s) => s.currentPath);
  const files = useStore((s) => s.files);
  const setFiles = useStore((s) => s.setFiles);
  const navigate = useStore((s) => s.navigate);
  const goBack = useStore((s) => s.goBack);
  const navIndex = useStore((s) => s.navIndex);
  const transfers = useStore((s) => s.transfers);
  const viewMode = useStore((s) => s.viewMode);
  const addBookmark = useStore((s) => s.addBookmark);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const pkg = browseTarget?.kind === "app" ? browseTarget.app.bundle_id : undefined;
  const fileNames = files.map((f) => f.name);
  const { selected, handleClick, selectAll, clearSelection } = useSelection(fileNames);
  useTransferListener();
  const { handleDrop, handleDragOver } = useFileDrop();
  useIosFileInfoListener();

  const [loadError, setLoadError] = useState<string | null>(null);
  const refreshedTransfers = useRef(new Set<string>());

  useEffect(() => {
    const terminalTransfers = transfers.filter(
      (task) =>
        task.status === "done" || task.status === "error" || task.status === "cancelled"
    );
    const pendingRefresh = terminalTransfers.some((task) => {
      if (refreshedTransfers.current.has(task.id)) return false;
      refreshedTransfers.current.add(task.id);
      return true;
    });
    if (pendingRefresh) {
      reloadFiles({ useCache: false });
    }
  }, [currentPath, device?.id, browseTarget, transfers]);

  // Latest-call-wins guard for async reloads: each reloadFiles call bumps the
  // sequence and discards older in-flight results. Covers both navigation
  // (effect cleanup bumps) and manual refresh — previously the refresh path
  // had no canceller, so its late-arriving listing could overwrite the
  // freshly navigated-to directory's files.
  const reloadSeq = useRef(0);

  function sleep(ms: number) {
    return new Promise((r) => setTimeout(r, ms));
  }

  function fetchList() {
    return device!.platform === "ios"
      ? tauriApi.listIosFiles(device!.id, pkg!, currentPath)
      : tauriApi.listAndroidFiles(device!.id, currentPath, pkg);
  }

  async function reloadFiles(opts: { useCache?: boolean } = { useCache: true }) {
    if (!device || !browseTarget) return;
    const mySeq = ++reloadSeq.current;
    const isStale = () => mySeq !== reloadSeq.current;
    const key = cacheKey(device.platform, device.id, pkg, currentPath);
    // Serve cached entries immediately so revisiting a directory is instant;
    // the fresh fetch below replaces them once it lands.
    setLoadError(null);
    const cached = opts.useCache ? listCache.get(key) : undefined;
    if (cached && !isStale()) {
      setFiles(cached);
      enqueueMissingIosInfo(cached);
    }
    // One retry on failure: rapid navigation can spawn several concurrent
    // afclient/adb subprocesses and occasionally one loses the race for the
    // USB/lockdownd connection; a short backoff usually recovers it.
    let failureMessage: string | null = null;
    const list = await fetchList().catch((first) =>
      sleep(400).then(fetchList).catch((second) => {
        failureMessage =
          second instanceof Error
            ? second.message
            : first instanceof Error
              ? first.message
              : "加载失败";
        console.error("Failed to load files:", first, second);
        return null;
      })
    );
    if (list && !isStale()) {
      setLoadError(null);
      // Fresh iOS entries carry placeholder metadata (is_dir:false, no
      // size/mtime) until probed. Replace each placeholder with the
      // already-probed metadata from cache when the entry existed before —
      // otherwise every background refresh would visually flip directories
      // back to file icons while probes re-run, and a quick navigation away
      // would persist the placeholders into the cache. Tradeoff: unchanged
      // files keep their last-known size/mtime; only genuinely new entries
      // get probed (enqueueMissingIosInfo below).
      const byPath = new Map((cached ?? []).map((c) => [c.path, c]));
      const merged = list.map((f) =>
        f.modified == null ? byPath.get(f.path) ?? f : f
      );
      setFiles(merged);
      listCache.set(key, merged);
      enqueueMissingIosInfo(merged);
    } else if (!isStale() && failureMessage) {
      setFiles([]);
      setLoadError(failureMessage);
    }
  }

  // Probe real metadata only for entries still carrying placeholders
  // (identified by missing `modified`) — cached entries that were already
  // probed on a previous visit skip the costly per-file afcclient calls.
  function enqueueMissingIosInfo(list: FileEntry[]) {
    if (!device || device.platform !== "ios") return;
    const needInfo = list.filter((f) => f.modified == null).map((f) => f.path);
    if (needInfo.length > 0) {
      tauriApi.enqueueIosFileInfo(device.id, pkg!, needInfo);
    }
  }

  useEffect(() => {
    let cancelled = false;
    reloadFiles().then(() => {
      if (!cancelled) {
        clearSelection();
      }
    });
    return () => {
      cancelled = true;
      // Invalidate any in-flight reload from this directory so its late
      // result never overwrites the newly navigated-to directory's files.
      reloadSeq.current++;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [device, browseTarget, currentPath]);

  // Write probed metadata back into the cache: patchFileInfo updates `files`
  // in place as iOS probes land, and without this those enriched entries
  // would be lost on the next visit (forcing a full re-probe). The path
  // guard prevents caching the previous directory's entries under the new
  // key during the render that follows a navigation.
  useEffect(() => {
    if (!device || !browseTarget || files.length === 0) return;
    const base = currentPath === "/" ? "" : currentPath.replace(/\/+$/, "");
    const matchesCurrentDir = files.every((f) => f.path === `${base}/${f.name}`);
    if (matchesCurrentDir) {
      listCache.set(cacheKey(device.platform, device.id, pkg, currentPath), files);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [files]);

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

  // Manual refresh: drop the cache for the current directory so the reload
  // neither serves nor merges stale metadata — sizes/mtimes are re-probed.
  function handleRefresh() {
    if (!device || !browseTarget) return;
    listCache.delete(cacheKey(device.platform, device.id, pkg, currentPath));
    reloadFiles({ useCache: false });
  }

  function handleAddBookmark() {
    if (!device || !browseTarget) return;
    addBookmark({
      platform: device.platform,
      app: browseTarget.kind === "app" ? browseTarget.app : null,
      path: currentPath,
    });
  }

  async function handleImport() {
    const paths = await open({ multiple: true });
    if (!paths || !device) return;
    const pathList = Array.isArray(paths) ? paths : [paths];
    const uploadFiles = pathList.map((localPath) => {
      const fileName = localPath.split("/").pop()!;
      return {
        src: localPath,
        dst: `${currentPath.replace(/\/$/, "")}/${fileName}`,
        is_dir: false,
      };
    });
    try {
      if (device.platform === "ios") {
        await tauriApi.enqueueIosUploadBatch(device.id, pkg!, uploadFiles);
      } else {
        await tauriApi.enqueueAndroidUploadBatch(device.id, uploadFiles, pkg);
      }
    } catch (e) {
      console.error("Failed to enqueue upload:", e);
    }
  }

  async function handleExport() {
    if (!device) return;
    const selectedFiles = files.filter((f) => selected.has(f.name));
    const destDir = await open({ directory: true });
    if (!destDir || typeof destDir !== "string") return;
    const downloadFiles = selectedFiles.map((file) => ({
      src: file.path,
      dst: `${destDir}/${file.name}`,
      is_dir: file.is_dir,
    }));
    try {
      if (device.platform === "ios") {
        await tauriApi.enqueueIosDownloadBatch(device.id, pkg!, downloadFiles);
      } else {
        await tauriApi.enqueueAndroidDownloadBatch(device.id, downloadFiles, pkg);
      }
    } catch (e) {
      console.error("Failed to enqueue download:", e);
    }
  }

  async function handleDelete() {
    if (!device || !window.confirm(`删除选中的 ${selected.size} 个文件？`)) return;
    const selectedFiles = files.filter((f) => selected.has(f.name));
    const remotePaths = selectedFiles.map((file) => file.path);
    try {
      if (device.platform === "ios") {
        await tauriApi.iosDeleteBatch(device.id, pkg!, remotePaths);
      } else {
        await tauriApi.androidDeleteBatch(device.id, remotePaths, pkg);
      }
    } catch (e) {
      console.error("Failed to enqueue delete:", e);
    }
    clearSelection();
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
        canGoBack={navIndex > 0}
        onBack={goBack}
        canGoUp={currentPath !== "/"}
        onUp={() => navigate(parentPath(currentPath))}
        onRefresh={handleRefresh}
        onBookmark={handleAddBookmark}
      />
      {loadError && (
        <div className="px-3 py-2 text-sm text-red-300 border-b border-red-900 bg-red-950/40">
          {loadError}
        </div>
      )}
      <div className="flex-1 overflow-auto">
        {viewMode === "list" ? (
          <FileList
            files={files}
            selected={selected}
            onNavigate={navigate}
            onSelect={handleClick}
          />
        ) : (
          <FileGrid
            files={files}
            selected={selected}
            onNavigate={navigate}
            onSelect={handleClick}
          />
        )}
      </div>
    </div>
  );
}
