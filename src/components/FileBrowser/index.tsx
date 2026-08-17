import { useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FileEntry, parentPath, useStore } from "../../store";
import { tauriApi, useIosFileInfoListener, useTransferListener } from "../../hooks/useTauri";
import { buildSearchIndex, compareSearchMatches, normalizeSearchQuery, rankSearchIndex } from "../../utils/search";
import { BreadcrumbBar } from "./BreadcrumbBar";
import { Toolbar } from "./Toolbar";
import { FileList } from "./FileList";
import { FileGrid } from "./FileGrid";
import { useSelection } from "./useSelection";
import { useFileDrop } from "./useFileDrop";
import { getFileBrowserShortcutAction } from "./shortcuts";
import { FileBrowserContextMenu } from "./FileBrowserContextMenu";

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
  const goForward = useStore((s) => s.goForward);
  const navIndex = useStore((s) => s.navIndex);
  const navHistory = useStore((s) => s.navHistory);
  const canGoBack = navIndex > 0;
  const canGoForward = navIndex < navHistory.length - 1;
  const viewMode = useStore((s) => s.viewMode);
  const bookmarks = useStore((s) => s.bookmarks);
  const addBookmark = useStore((s) => s.addBookmark);
  const removeBookmark = useStore((s) => s.removeBookmark);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const pkg = browseTarget?.kind === "app" ? browseTarget.app.bundle_id : undefined;
  const [fileSearch, setFileSearch] = useState("");
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; actions: { label: string; onAction: () => void }[] } | null>(null);
  const searchTerm = normalizeSearchQuery(fileSearch);
  const visibleFiles = useMemo(
    () =>
      searchTerm
        ? files
            .map((file) => ({ file, match: rankSearchIndex(file.search_index ?? buildSearchIndex(file.name), searchTerm) }))
            .filter((entry): entry is { file: FileEntry; match: NonNullable<typeof entry.match> } => entry.match != null)
            .sort((a, b) => compareSearchMatches(a.match, b.match) || a.file.name.localeCompare(b.file.name))
            .map((entry) => entry.file)
        : files,
    [files, searchTerm],
  );
  const visibleFileNames = visibleFiles.map((f) => f.name);
  const { selected, handleClick, selectOnly, selectAll, clearSelection } = useSelection(visibleFileNames);
  const selectedVisibleFiles = visibleFiles.filter((file) => selected.has(file.name));
  const { handleDrop, handleDragOver } = useFileDrop();

  useTransferListener();
  useIosFileInfoListener();

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
    const cached = opts.useCache ? listCache.get(key) : undefined;
    if (cached && !isStale()) {
      setFiles(cached);
      enqueueMissingIosInfo(cached);
    }
    // One retry on failure: rapid navigation can spawn several concurrent
    // afclient/adb subprocesses and occasionally one loses the race for the
    // USB/lockdownd connection; a short backoff usually recovers it.
    const list = await fetchList().catch((first) =>
      sleep(400).then(fetchList).catch((second) => {
        console.error("Failed to load files:", first, second);
        return null;
      })
    );
    if (list && !isStale()) {
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
    setFileSearch("");
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
      const shortcut = getFileBrowserShortcutAction(e);
      if (!shortcut) return;

      e.preventDefault();
      if ((shortcut === "download" || shortcut === "delete") && selected.size === 0) return;

      switch (shortcut) {
        case "back":
          if (canGoBack) goBack();
          break;
        case "forward":
          if (canGoForward) goForward();
          break;
        case "up":
          if (currentPath !== "/") navigate(parentPath(currentPath));
          break;
        case "bookmark":
          handleToggleBookmark();
          break;
        case "upload":
          void handleImport();
          break;
        case "download":
          void handleExport();
          break;
        case "delete":
          void handleDelete();
          break;
        case "select-all":
          selectAll();
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    canGoBack,
    canGoForward,
    currentPath,
    goBack,
    goForward,
    handleDelete,
    handleExport,
    handleImport,
    handleToggleBookmark,
    navigate,
    selectAll,
    selected.size,
  ]);

  // Manual refresh: drop the cache for the current directory so the reload
  // neither serves nor merges stale metadata — sizes/mtimes are re-probed.
  function handleRefresh() {
    if (!device || !browseTarget) return;
    listCache.delete(cacheKey(device.platform, device.id, pkg, currentPath));
    reloadFiles({ useCache: false });
  }

  function handleToggleBookmark() {
    if (!device || !browseTarget) return;
    const bookmark = {
      platform: device.platform,
      app: browseTarget.kind === "app" ? browseTarget.app : null,
      path: currentPath,
    };
    const exists = bookmarks.some(
      (b) =>
        b.platform === bookmark.platform &&
        (b.app?.bundle_id ?? "") === (bookmark.app?.bundle_id ?? "") &&
        b.path === bookmark.path,
    );
    if (exists) {
      removeBookmark(bookmark);
      return;
    }
    addBookmark(bookmark);
  }

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

  function handleBackgroundContextMenu(event: ReactMouseEvent<HTMLDivElement>) {
    const target = event.target as HTMLElement | null;
    if (target?.closest("[data-file-entry]")) return;
    event.preventDefault();
    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      actions: [{ label: "导入", onAction: handleImport }],
    });
  }

  function handleFileContextMenu(name: string, event: ReactMouseEvent) {
    const target = event.target as HTMLElement | null;
    if (!selected.has(name)) {
      selectOnly(name);
    }
    if (target?.closest("[data-file-entry]")) {
      event.preventDefault();
      setContextMenu({
        x: event.clientX,
        y: event.clientY,
        actions: [
          { label: "导出", onAction: handleExport },
          { label: "删除", onAction: handleDelete },
        ],
      });
    }
  }

  async function handleExport() {
    if (!device) return;
    const selectedFiles = selectedVisibleFiles;
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

  async function handleDelete() {    if (!device || !window.confirm(`删除选中的 ${selectedVisibleFiles.length} 个文件？`)) return;
    const selectedFiles = selectedVisibleFiles;
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
        selectedCount={selectedVisibleFiles.length}
        onImport={handleImport}
        onExport={handleExport}
        onDelete={handleDelete}
        canGoBack={canGoBack}
        onBack={goBack}
        canGoForward={canGoForward}
        onForward={goForward}
        canGoUp={currentPath !== "/"}
        onUp={() => navigate(parentPath(currentPath))}
        onRefresh={handleRefresh}
        onBookmark={handleToggleBookmark}
        searchValue={fileSearch}
        onSearchChange={setFileSearch}
      />
      <div
        className="flex-1 overflow-auto"
        aria-label="文件浏览区域"
        onContextMenu={handleBackgroundContextMenu}
      >
        {viewMode === "list" ? (
          <FileList
            files={visibleFiles}
            selected={selected}
            onNavigate={navigate}
            onSelect={handleClick}
            onContextMenu={handleFileContextMenu}
          />
        ) : (
          <FileGrid
            files={visibleFiles}
            selected={selected}
            onNavigate={navigate}
            onSelect={handleClick}
            onContextMenu={handleFileContextMenu}
          />
        )}
      </div>
      {contextMenu && (
        <FileBrowserContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          actions={contextMenu.actions}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}
