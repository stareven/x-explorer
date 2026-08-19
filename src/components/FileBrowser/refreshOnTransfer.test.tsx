import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { FileBrowser } from "./index";
import { FileEntry, useStore } from "../../store";
import { tauriApi, type TransferProgress } from "../../hooks/useTauri";

// Mock only the Tauri event bus so we can capture and fire `transfer-progress`
// events ourselves; everything else (including `useTransferListener` itself)
// runs real code. Module-level `vi.mock` is scoped per test file, so this
// file does not affect the mocks of the sibling FileBrowser tests.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

// Re-mock the Tauri command surface so each method is a vi.fn() that the
// tests can both stub responses on and observe calls on. We deliberately do
// NOT mock `useTransferListener` itself — the whole point of these tests is
// to drive the real listener wiring through FileBrowser. `vi.importActual`
// lets us keep the real exports (including `useTransferListener`) while
// replacing `tauriApi` with stub functions. The platform-dispatch helpers
// delegate to the per-platform mocks so assertions on those continue to work.
vi.mock("../../hooks/useTauri", async () => {
  const actual = await vi.importActual<typeof import("../../hooks/useTauri")>("../../hooks/useTauri");
  const tauriApi = {
    listIosDevices: vi.fn(),
    listAndroidDevices: vi.fn(),
    listIosApps: vi.fn(),
    listAndroidApps: vi.fn(),
    listIosFiles: vi.fn(),
    enqueueIosFileInfo: vi.fn(),
    listAndroidFiles: vi.fn(),
    iosDelete: vi.fn(),
    iosDeleteBatch: vi.fn(),
    enqueueIosDeleteDir: vi.fn(),
    androidDelete: vi.fn(),
    androidDeleteBatch: vi.fn(),
    enqueueIosDownload: vi.fn(),
    enqueueIosDownloadBatch: vi.fn(),
    enqueueIosUpload: vi.fn(),
    enqueueIosUploadBatch: vi.fn(),
    enqueueAndroidDownload: vi.fn(),
    enqueueAndroidDownloadBatch: vi.fn(),
    enqueueAndroidUpload: vi.fn(),
    enqueueAndroidUploadBatch: vi.fn(),
    cancelTransfer: vi.fn(),
  };
  return {
    ...actual,
    tauriApi,
    enqueueUploadBatch: vi.fn((device: { platform: string; id: string }, pkg: string | undefined, files: unknown[]) =>
      device.platform === "ios"
        ? tauriApi.enqueueIosUploadBatch(device.id, pkg!, files as never)
        : tauriApi.enqueueAndroidUploadBatch(device.id, files as never, pkg)
    ),
    enqueueDeleteBatch: vi.fn((device: { platform: string; id: string }, pkg: string | undefined, paths: string[]) =>
      device.platform === "ios"
        ? tauriApi.iosDeleteBatch(device.id, pkg!, paths)
        : tauriApi.androidDeleteBatch(device.id, paths, pkg)
    ),
    enqueueDeleteDir: vi.fn((device: { platform: string; id: string }, pkg: string | undefined, path: string) =>
      device.platform === "ios"
        ? tauriApi.enqueueIosDeleteDir(device.id, pkg!, path)
        : tauriApi.androidDelete(device.id, path, pkg)
    ),
  };
});

// Render-time companions of FileBrowser — replaced with per-test stubs.
vi.mock("./BreadcrumbBar", () => ({
  BreadcrumbBar: () => <div data-testid="breadcrumb-bar" />,
}));
vi.mock("./Toolbar", () => ({
  Toolbar: () => <div data-testid="toolbar" />,
}));
vi.mock("./FileList", () => ({
  FileList: () => <div data-testid="file-list" />,
}));
vi.mock("./FileGrid", () => ({
  FileGrid: () => <div data-testid="file-grid" />,
}));
vi.mock("./useFileDrop", () => ({
  useFileDrop: () => ({ handleDrop: vi.fn(), handleDragOver: vi.fn() }),
}));

// Default: empty selection; individual tests call `mockSelection` to populate.
vi.mock("./useSelection", () => ({
  useSelection: vi.fn(() => ({
    selected: new Set<string>(),
    handleClick: vi.fn(),
    selectOnly: vi.fn(),
    selectAll: vi.fn(),
    clearSelection: vi.fn(),
  })),
}));

import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useSelection } from "./useSelection";

type Handler = (event: { payload: TransferProgress }) => void;

// Setup `listen` to resolve to a no-op unlisten. The handler is recovered
// directly from the mock's call history — keeping state in module-scoped
// variables would race with the effect cleanups that fire whenever the
// listener re-subscribes (and our `useTransferListener` does re-subscribe
// every render, since its `onTaskComplete` prop is an inline arrow).
vi.mocked(listen).mockResolvedValue(() => {});

function getHandler(event: string): Handler {
  // Return the *last* registered handler — `useTransferListener`'s effect
  // re-runs every render (its `onTaskComplete` prop is an inline arrow), so
  // early registrations are stale closures we should not invoke.
  for (let i = vi.mocked(listen).mock.calls.length - 1; i >= 0; i--) {
    const args = vi.mocked(listen).mock.calls[i];
    if (args[0] === event) return args[1] as Handler;
  }
  throw new Error(`${event} listener was never registered`);
}

function emitTransfer(task: Partial<TransferProgress> & Pick<TransferProgress, "task_id" | "kind" | "status">) {
  const handler = getHandler("transfer-progress");
  const payload: TransferProgress = {
    src: "",
    dst: "",
    total_files: 1,
    completed_files: 0,
    error: undefined,
    ...task,
  };
  act(() => {
    handler({ payload });
  });
}

function mockSelection(names: string[]) {
  vi.mocked(useSelection).mockReturnValue({
    selected: new Set(names),
    handleClick: vi.fn(),
    selectOnly: vi.fn(),
    selectAll: vi.fn(),
    clearSelection: vi.fn(),
  });
}

const initialFiles: FileEntry[] = [
  { name: "a.txt", path: "/a.txt", is_dir: false, size: 1, modified: 1 },
];

const device = { id: "device-1", name: "iPhone", platform: "ios" as const, status: "connected" as const };
const app = { name: "App", bundle_id: "com.example.app" };

// Project the store's `files` to just (name, path) so the assertions stay
// readable; `setFiles` enriches entries with a `search_index` that would
// otherwise leak into every equality check.
const fileNames = () => useStore.getState().files.map((f) => f.name);

describe("FileBrowser refresh on transfer completion", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(useSelection).mockReturnValue({
      selected: new Set<string>(),
      handleClick: vi.fn(),
      selectOnly: vi.fn(),
      selectAll: vi.fn(),
      clearSelection: vi.fn(),
    });
    Object.defineProperty(window, "localStorage", {
      value: { getItem: vi.fn(() => null), setItem: vi.fn(), removeItem: vi.fn(), clear: vi.fn() },
      configurable: true,
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);
    useStore.setState({
      devices: [device],
      selectedDeviceId: device.id,
      selectedApp: app,
      browseTarget: { kind: "app", app },
      currentPath: "/",
      navHistory: ["/"],
      navIndex: 0,
      files: [],
      transfers: [],
      viewMode: "list",
      bookmarks: [],
      favoriteAppIds: [],
    });
    vi.mocked(tauriApi.listIosFiles).mockResolvedValue(initialFiles);
    vi.mocked(tauriApi.listAndroidFiles).mockResolvedValue(initialFiles);
  });

  it("reloads the directory after a dialog upload completes", async () => {
    vi.mocked(open).mockResolvedValue(["/tmp/doc.pdf"] as never);
    vi.mocked(tauriApi.enqueueIosUploadBatch).mockResolvedValue("upload-dialog-1");
    vi.mocked(tauriApi.enqueueIosFileInfo).mockResolvedValue(undefined as never);
    // First list: initial render. Second: post-completion reload.
    vi.mocked(tauriApi.listIosFiles)
      .mockResolvedValueOnce(initialFiles)
      .mockResolvedValueOnce([
        ...initialFiles,
        { name: "doc.pdf", path: "/doc.pdf", is_dir: false, size: 5, modified: 2 },
      ]);

    render(<FileBrowser />);
    await waitFor(() => expect(fileNames()).toEqual(["a.txt"]));

    // Drive the real handleImport path through Cmd+U so the matching
    // `rememberPendingReload` actually runs (calling `enqueueIosUploadBatch`
    // directly would skip it and make the wiring under test invisible).
    fireEvent.keyDown(window, { key: "u", metaKey: true });
    await waitFor(() => expect(tauriApi.enqueueIosUploadBatch).toHaveBeenCalled());
    // `handleImport` is fire-and-forget from the keyboard handler. Let its
    // post-await `rememberPendingReload(taskId)` line run before we emit the
    // matching `done` event, otherwise the listener's reload check fires
    // against an empty `pendingReloadCtxRef` and silently skips.
    await new Promise((r) => setTimeout(r, 30));

    vi.mocked(tauriApi.listIosFiles).mockClear();
    emitTransfer({ task_id: "upload-dialog-1", kind: "upload", status: "done", total_files: 1, completed_files: 1 });

    await waitFor(() => expect(vi.mocked(tauriApi.listIosFiles)).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(fileNames()).toContain("doc.pdf"));
  });

  it("reloads the directory after a delete batch completes", async () => {
    mockSelection(["a.txt"]);
    vi.mocked(tauriApi.iosDeleteBatch).mockResolvedValue("delete-task-1");
    vi.mocked(tauriApi.listIosFiles)
      .mockResolvedValueOnce(initialFiles)
      .mockResolvedValueOnce([]);

    render(<FileBrowser />);
    await waitFor(() => expect(fileNames()).toEqual(["a.txt"]));

    // Drive through real handleDelete (Cmd+Backspace) for the same reason as
    // above: only then does `rememberPendingReload` get called.
    fireEvent.keyDown(window, { key: "Backspace", metaKey: true });
    await waitFor(() => expect(tauriApi.iosDeleteBatch).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 30));

    vi.mocked(tauriApi.listIosFiles).mockClear();
    emitTransfer({ task_id: "delete-task-1", kind: "delete", status: "done", total_files: 1, completed_files: 1 });

    await waitFor(() => expect(vi.mocked(tauriApi.listIosFiles)).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(fileNames()).toEqual([]));
  });

  it("routes mixed file and directory selections to their respective delete APIs", async () => {
    const mixedFiles: FileEntry[] = [
      { name: "a.txt", path: "/a.txt", is_dir: false, size: 1, modified: 2 },
      { name: "b.txt", path: "/b.txt", is_dir: false, size: 2, modified: 3 },
      { name: "folder", path: "/folder", is_dir: true, size: 0, modified: 1 },
    ];
    mockSelection(["a.txt", "b.txt", "folder"]);
    vi.mocked(tauriApi.listIosFiles).mockResolvedValue(mixedFiles);

    render(<FileBrowser />);
    await waitFor(() => expect(fileNames()).toEqual(["a.txt", "b.txt", "folder"]));

    fireEvent.keyDown(window, { key: "Backspace", metaKey: true });

    await waitFor(() => {
      expect(vi.mocked(tauriApi.enqueueIosDeleteDir)).toHaveBeenCalledWith(
        device.id,
        app.bundle_id,
        "/folder"
      );
      expect(vi.mocked(tauriApi.iosDeleteBatch)).toHaveBeenCalledWith(
        device.id,
        app.bundle_id,
        ["/a.txt", "/b.txt"]
      );
    });
  });

  it("does not reload after a download task completes", async () => {
    // Even if the listener called `reloadFiles` on a download, no `pendingReloadCtx`
    // was ever set (handleExport does not call `rememberPendingReload`), so no
    // reload will fire regardless of the recorded task id.
    vi.mocked(open).mockResolvedValue("/tmp/export" as never);
    vi.mocked(tauriApi.enqueueIosDownloadBatch).mockResolvedValue("download-task-1");

    mockSelection(["a.txt"]);
    render(<FileBrowser />);
    await waitFor(() => expect(fileNames()).toEqual(["a.txt"]));

    fireEvent.keyDown(window, { key: "s", metaKey: true });
    await waitFor(() => expect(tauriApi.enqueueIosDownloadBatch).toHaveBeenCalled());

    vi.mocked(tauriApi.listIosFiles).mockClear();
    emitTransfer({ task_id: "download-task-1", kind: "download", status: "done", total_files: 1, completed_files: 1 });

    await new Promise((r) => setTimeout(r, 30));
    expect(vi.mocked(tauriApi.listIosFiles)).not.toHaveBeenCalled();
  });

  it("does not refresh the destination directory if the user navigated away mid-transfer", async () => {
    vi.mocked(open).mockResolvedValue(["/tmp/doc.pdf"] as never);
    vi.mocked(tauriApi.enqueueIosUploadBatch).mockResolvedValue("upload-nav-1");
    vi.mocked(tauriApi.enqueueIosFileInfo).mockResolvedValue(undefined as never);

    render(<FileBrowser />);
    await waitFor(() => expect(fileNames()).toEqual(["a.txt"]));

    fireEvent.keyDown(window, { key: "u", metaKey: true });
    await waitFor(() => expect(tauriApi.enqueueIosUploadBatch).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 30));

    // Navigate to `/sub` before the backend reports `done`. The listener
    // must not clobber the newly navigated-to directory's listing with a
    // stale refresh of the original one.
    act(() => {
      useStore.getState().navigate("/sub");
    });
    await waitFor(() => expect(useStore.getState().currentPath).toBe("/sub"));

    vi.mocked(tauriApi.listIosFiles).mockClear();
    emitTransfer({ task_id: "upload-nav-1", kind: "upload", status: "done", total_files: 1, completed_files: 1 });

    await new Promise((r) => setTimeout(r, 30));
    const calls = vi.mocked(tauriApi.listIosFiles).mock.calls;
    expect(calls.every(([, , path]) => path === "/sub")).toBe(true);
  });
});
