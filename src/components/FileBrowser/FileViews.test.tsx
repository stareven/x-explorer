import { beforeEach, describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AppList } from "../DevicePanel/AppList";
import { FileList } from "./FileList";
import { Device, FileEntry, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

vi.mock("../../hooks/useTauri", () => ({
  tauriApi: {
    listAndroidApps: vi.fn(),
    listAndroidFiles: vi.fn().mockResolvedValue([]),
  },
  useTransferListener: vi.fn(),
  useIosFileInfoListener: vi.fn(),
}));

const mockFiles: FileEntry[] = [
  { name: "Documents", path: "/Documents", is_dir: true, size: 0 },
  { name: "config.json", path: "/config.json", is_dir: false, size: 1024, modified: 1710000000 },
];

beforeEach(() => {
  const devices: Device[] = [
    { id: "device-1", name: "Pixel", platform: "android", status: "connected" },
  ];
  useStore.setState({
    devices,
    selectedDeviceId: "device-1",
    browseTarget: null,
    currentPath: "/",
    navHistory: ["/"],
    navIndex: 0,
    selectedApp: null,
    files: [],
    transfers: [],
    viewMode: "list",
    favoriteAppIds: [],
    bookmarks: [],
  });
  vi.clearAllMocks();
});

describe("FileList", () => {
  it("renders file names", () => {
    const onNavigate = vi.fn();
    const onSelect = vi.fn();
    render(
      <FileList
        files={mockFiles}
        selected={new Set()}
        onNavigate={onNavigate}
        onSelect={onSelect}
        onContextMenu={vi.fn()}
      />
    );
    expect(screen.getByText("Documents")).toBeInTheDocument();
    expect(screen.getByText("config.json")).toBeInTheDocument();
  });

  it("calls onNavigate when clicking a directory", () => {
    const onNavigate = vi.fn();
    render(
      <FileList
        files={mockFiles}
        selected={new Set()}
        onNavigate={onNavigate}
        onSelect={vi.fn()}
        onContextMenu={vi.fn()}
      />
    );
    fireEvent.dblClick(screen.getByText("Documents"));
    expect(onNavigate).toHaveBeenCalledWith("/Documents");
  });

  it("shows file size for files", () => {
    render(
      <FileList
        files={mockFiles}
        selected={new Set()}
        onNavigate={vi.fn()}
        onSelect={vi.fn()}
        onContextMenu={vi.fn()}
      />
    );
    expect(screen.getByText("1.0 KB")).toBeInTheDocument();
  });

  it("does not show android app names in the sidebar", async () => {
    render(<AppList />);

    expect(screen.getAllByText("外部存储")).toHaveLength(2);
    expect(screen.queryByText("Baidu Maps")).toBeNull();
    expect(vi.mocked(tauriApi.listAndroidApps)).not.toHaveBeenCalled();
  });
});

