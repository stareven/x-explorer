import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { FileBrowser } from "./index";
import { useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";
import { open } from "@tauri-apps/plugin-dialog";
import { useSelection } from "./useSelection";

vi.mock("../../hooks/useTauri", () => {
  const tauriApi = {
    listIosFiles: vi.fn().mockResolvedValue([]),
    listAndroidFiles: vi.fn().mockResolvedValue([]),
    enqueueIosFileInfo: vi.fn().mockResolvedValue(undefined),
    enqueueIosUpload: vi.fn().mockResolvedValue(undefined),
    enqueueIosUploadBatch: vi.fn().mockResolvedValue(undefined),
    enqueueAndroidUpload: vi.fn().mockResolvedValue(undefined),
    enqueueAndroidUploadBatch: vi.fn().mockResolvedValue(undefined),
    enqueueIosDownload: vi.fn().mockResolvedValue(undefined),
    enqueueIosDownloadBatch: vi.fn().mockResolvedValue(undefined),
    enqueueAndroidDownload: vi.fn().mockResolvedValue(undefined),
    enqueueAndroidDownloadBatch: vi.fn().mockResolvedValue(undefined),
    iosDeleteBatch: vi.fn().mockResolvedValue(undefined),
    enqueueIosDeleteDir: vi.fn().mockResolvedValue(undefined),
    androidDelete: vi.fn().mockResolvedValue(undefined),
    androidDeleteBatch: vi.fn().mockResolvedValue(undefined),
  };
  return {
    tauriApi,
    // Platform-dispatch helpers delegate to the per-platform mocks above so
    // assertions on `tauriApi.enqueueIosUploadBatch` etc. continue to work.
    enqueueUploadBatch: vi.fn((device, pkg, files) =>
      device.platform === "ios"
        ? tauriApi.enqueueIosUploadBatch(device.id, pkg, files)
        : tauriApi.enqueueAndroidUploadBatch(device.id, files, pkg)
    ),
    enqueueDeleteBatch: vi.fn((device, pkg, paths) =>
      device.platform === "ios"
        ? tauriApi.iosDeleteBatch(device.id, pkg, paths)
        : tauriApi.androidDeleteBatch(device.id, paths, pkg)
    ),
    enqueueDeleteDir: vi.fn((device, pkg, path) =>
      device.platform === "ios"
        ? tauriApi.enqueueIosDeleteDir(device.id, pkg, path)
        : tauriApi.androidDelete(device.id, path, pkg)
    ),
    useIosFileInfoListener: vi.fn(),
    useTransferListener: vi.fn(),
  };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("./BreadcrumbBar", () => ({
  BreadcrumbBar: () => <div data-testid="breadcrumb-bar" />,
}));

vi.mock("./Toolbar", () => ({
  Toolbar: () => <div data-testid="toolbar" />,
}));

vi.mock("./FileList", () => ({
  FileList: (props: { onContextMenu?: (name: string, event: ReactMouseEvent<HTMLDivElement>) => void }) => (
    <div data-testid="file-list">
      <div
        data-file-entry
        data-testid="file-entry"
        onContextMenu={(event) => props.onContextMenu?.("one.txt", event)}
      />
    </div>
  ),
}));

vi.mock("./FileGrid", () => ({
  FileGrid: () => <div data-testid="file-grid" />,
}));

vi.mock("./useFileDrop", () => ({
  useFileDrop: vi.fn(() => ({
    handleDrop: vi.fn(),
    handleDragOver: vi.fn(),
  })),
}));

vi.mock("./useSelection", () => ({
  useSelection: vi.fn(),
}));

const mockFiles = [
  { name: "one.txt", path: "/Documents/one.txt", is_dir: false, size: 1, modified: 1 },
  { name: "two.txt", path: "/Documents/two.txt", is_dir: false, size: 2, modified: 2 },
];

const iosDevice = { id: "dev-1", name: "iPhone", platform: "ios" as const, status: "connected" as const };
const app = { bundle_id: "com.example.app", name: "Example" };

function setBrowserState() {
  useStore.setState({
    devices: [iosDevice],
    selectedDeviceId: iosDevice.id,
    selectedApp: app,
    browseTarget: { kind: "app", app },
    currentPath: "/Documents",
    navHistory: ["/", "/Documents"],
    navIndex: 1,
    files: mockFiles,
    viewMode: "list",
    bookmarks: [],
    favoriteAppIds: [],
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

describe("FileBrowser shortcuts", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn(() => null),
        setItem: vi.fn(),
        removeItem: vi.fn(),
        clear: vi.fn(),
      },
      configurable: true,
    });
    setBrowserState();
    mockSelection([]);
    vi.mocked(tauriApi.listIosFiles).mockResolvedValue(mockFiles as never);
    vi.mocked(open).mockResolvedValue(null as never);
    vi.spyOn(window, "confirm").mockReturnValue(true);
  });

  it("navigates back, forward, and up from keyboard shortcuts", async () => {
    render(<FileBrowser />);

    fireEvent.keyDown(window, { key: "[", metaKey: true });
    expect(useStore.getState().currentPath).toBe("/");

    fireEvent.keyDown(window, { key: "]", metaKey: true });
    expect(useStore.getState().currentPath).toBe("/Documents");

    fireEvent.keyDown(window, { key: "ArrowUp", metaKey: true });
    expect(useStore.getState().currentPath).toBe("/");
  });

  it("toggles a bookmark from Cmd+B", () => {
    render(<FileBrowser />);

    fireEvent.keyDown(window, { key: "b", metaKey: true });
    expect(useStore.getState().bookmarks).toEqual([{ platform: "ios", app, path: "/Documents" }]);

    fireEvent.keyDown(window, { key: "b", metaKey: true });
    expect(useStore.getState().bookmarks).toEqual([]);
  });

  it("uploads from Cmd+U", async () => {
    vi.mocked(open).mockResolvedValue(["/tmp/report.pdf"] as never);
    render(<FileBrowser />);

    fireEvent.keyDown(window, { key: "u", metaKey: true });

    await waitFor(() => {
      expect(vi.mocked(tauriApi.enqueueIosUploadBatch)).toHaveBeenCalledWith(
        iosDevice.id,
        app.bundle_id,
        [{ src: "/tmp/report.pdf", dst: "/Documents/report.pdf", is_dir: false }]
      );
    });
    expect(vi.mocked(tauriApi.enqueueIosUpload)).not.toHaveBeenCalled();
  });

  it("opens an import-only context menu from the blank browser background", async () => {
    vi.mocked(open).mockResolvedValue(["/tmp/context.pdf"] as never);
    render(<FileBrowser />);

    fireEvent.contextMenu(screen.getByLabelText("文件浏览区域"), { clientX: 120, clientY: 160 });

    expect(screen.getByRole("menuitem", { name: "导入" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "导出" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "删除" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("menuitem", { name: "导入" }));

    await waitFor(() => {
      expect(vi.mocked(tauriApi.enqueueIosUploadBatch)).toHaveBeenCalledWith(
        iosDevice.id,
        app.bundle_id,
        [{ src: "/tmp/context.pdf", dst: "/Documents/context.pdf", is_dir: false }]
      );
    });
    expect(vi.mocked(tauriApi.enqueueIosUpload)).not.toHaveBeenCalled();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("opens export and delete actions from a selected file row", async () => {
    render(<FileBrowser />);

    fireEvent.contextMenu(screen.getByTestId("file-entry"), { clientX: 120, clientY: 160 });

    await waitFor(() => {
      expect(screen.getByRole("menuitem", { name: "导出" })).toBeInTheDocument();
    });
    expect(screen.getByRole("menuitem", { name: "删除" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "导入" })).not.toBeInTheDocument();
  });

  it("opens the import context menu from blank space inside the file list", async () => {
    render(<FileBrowser />);

    fireEvent.contextMenu(screen.getByTestId("file-list"), { clientX: 120, clientY: 160 });

    await waitFor(() => {
      expect(screen.getByRole("menuitem", { name: "导入" })).toBeInTheDocument();
    });
  });

  it("downloads selected files from Cmd+S", async () => {
    mockSelection(["one.txt"]);
    vi.mocked(open).mockResolvedValue("/tmp/export" as never);
    render(<FileBrowser />);

    fireEvent.keyDown(window, { key: "s", metaKey: true });

    await waitFor(() => {
      expect(vi.mocked(tauriApi.enqueueIosDownloadBatch)).toHaveBeenCalledWith(
        iosDevice.id,
        app.bundle_id,
        [{ src: "/Documents/one.txt", dst: "/tmp/export/one.txt", is_dir: false }]
      );
    });
    expect(vi.mocked(tauriApi.enqueueIosDownload)).not.toHaveBeenCalled();
  });

  it("downloads a directory via batch API with is_dir=true so progress tracks leaf files", async () => {
    vi.mocked(tauriApi.listIosFiles).mockResolvedValue([
      { name: "two.txt", path: "/Documents/two.txt", is_dir: true, size: 0, modified: 1 },
    ] as never);
    mockSelection(["two.txt"]);
    vi.mocked(open).mockResolvedValue("/tmp/export" as never);
    render(<FileBrowser />);

    // Render-time useEffect calls reloadFiles() which overwrites the store;
    // wait for the mocked directory entry to land before exporting.
    await waitFor(() => {
      expect(useStore.getState().files).toEqual([
        expect.objectContaining({ name: "two.txt", is_dir: true }),
      ]);
    });

    fireEvent.keyDown(window, { key: "s", metaKey: true });

    await waitFor(() => {
      expect(vi.mocked(tauriApi.enqueueIosDownloadBatch)).toHaveBeenCalledWith(
        iosDevice.id,
        app.bundle_id,
        [{ src: "/Documents/two.txt", dst: "/tmp/export/two.txt", is_dir: true }]
      );
    });
    expect(vi.mocked(tauriApi.enqueueIosDownload)).not.toHaveBeenCalled();
  });

  it("does not download or delete without a selection", () => {
    render(<FileBrowser />);

    fireEvent.keyDown(window, { key: "s", metaKey: true });
    fireEvent.keyDown(window, { key: "Backspace", metaKey: true });

    expect(vi.mocked(open)).not.toHaveBeenCalled();
    expect(vi.mocked(tauriApi.enqueueIosDownload)).not.toHaveBeenCalled();
    expect(vi.mocked(tauriApi.enqueueIosDownloadBatch)).not.toHaveBeenCalled();
    expect(vi.mocked(tauriApi.iosDeleteBatch)).not.toHaveBeenCalled();
  });

  it("deletes selected files from Cmd+Backspace", async () => {
    mockSelection(["one.txt"]);
    render(<FileBrowser />);

    fireEvent.keyDown(window, { key: "Backspace", metaKey: true });

    await waitFor(() => {
      expect(window.confirm).toHaveBeenCalledWith("删除选中的 1 个文件？");
      expect(vi.mocked(tauriApi.iosDeleteBatch)).toHaveBeenCalledWith(
        iosDevice.id,
        app.bundle_id,
        ["/Documents/one.txt"]
      );
    });
  });
});
