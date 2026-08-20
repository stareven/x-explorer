import { beforeEach, describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { FileList } from "./FileList";
import { FileGrid } from "./FileGrid";
import { Device, FileEntry, useStore } from "../../store";

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

  it("renders data-file-name on each row", () => {
    render(
      <FileList
        files={mockFiles}
        selected={new Set()}
        onNavigate={vi.fn()}
        onSelect={vi.fn()}
        onContextMenu={vi.fn()}
      />
    );
    const rows = document.querySelectorAll("[data-file-entry]");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute("data-file-name", "Documents");
    expect(rows[1]).toHaveAttribute("data-file-name", "config.json");
  });
});

describe("FileGrid", () => {
  it("renders data-file-name on each item", () => {
    render(
      <FileGrid
        files={mockFiles}
        selected={new Set()}
        onNavigate={vi.fn()}
        onSelect={vi.fn()}
        onContextMenu={vi.fn()}
      />
    );
    const items = document.querySelectorAll("[data-file-entry]");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveAttribute("data-file-name", "Documents");
    expect(items[1]).toHaveAttribute("data-file-name", "config.json");
  });
});

