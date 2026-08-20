import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { FileBrowser } from "./index";
import { FileEntry, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../../hooks/useTauri", () => ({
  tauriApi: {
    listIosFiles: vi.fn(),
    listAndroidFiles: vi.fn(),
    enqueueIosFileInfo: vi.fn(),
    enqueueIosDownload: vi.fn(),
    enqueueAndroidDownload: vi.fn(),
    enqueueIosUpload: vi.fn(),
    enqueueAndroidUpload: vi.fn(),
    androidDelete: vi.fn(),
  },
  useTransferListener: vi.fn(),
  useIosFileInfoListener: vi.fn(),
}));

const files: FileEntry[] = [
  { name: "Alpha.txt", path: "/Alpha.txt", is_dir: false, size: 100 },
  { name: "beta-notes.md", path: "/beta-notes.md", is_dir: false, size: 200 },
  { name: "照片备份", path: "/照片备份", is_dir: false, size: 0 },
  { name: "Gamma", path: "/Gamma", is_dir: true, size: 0 },
];

const device = { id: "device-1", name: "iPhone", platform: "ios" as const, status: "connected" as const };
const app = { name: "App", bundle_id: "com.example.app" };

function domRect(left: number, top: number, width: number, height: number): DOMRect {
  return {
    left,
    top,
    width,
    height,
    right: left + width,
    bottom: top + height,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

beforeEach(() => {
  vi.clearAllMocks();
  useStore.setState({
    devices: [device],
    selectedDeviceId: device.id,
    selectedApp: app,
    browseTarget: { kind: "app", app },
    currentPath: "/",
    navHistory: ["/"],
    navIndex: 0,
    files: [],
    viewMode: "list",
  });
  vi.mocked(tauriApi.listIosFiles).mockResolvedValue(files);
  vi.mocked(tauriApi.listAndroidFiles).mockResolvedValue(files);
});

describe("FileBrowser search", () => {
  it("filters files by filename while typing", async () => {
    render(<FileBrowser />);

    await waitFor(() => expect(screen.getByText("Alpha.txt")).toBeInTheDocument());

    const input = screen.getByPlaceholderText("搜索文件");
    fireEvent.change(input, { target: { value: "beta" } });

    expect(screen.queryByText("Alpha.txt")).not.toBeInTheDocument();
    expect(screen.getByText("beta-notes.md")).toBeInTheDocument();
    expect(screen.queryByText("Gamma")).not.toBeInTheDocument();
  });

  it("orders file search results by match quality", async () => {
    vi.mocked(tauriApi.listIosFiles).mockResolvedValue([
      { name: "我的照片", path: "/我的照片", is_dir: false, size: 0 },
      { name: "照片", path: "/照片", is_dir: false, size: 0 },
      { name: "照片备份", path: "/照片备份", is_dir: false, size: 0 },
    ]);

    render(<FileBrowser />);

    await waitFor(() => expect(screen.getByText("我的照片")).toBeInTheDocument());
    fireEvent.change(screen.getByPlaceholderText("搜索文件"), { target: { value: "照片" } });

    const rows = screen.getAllByRole("row").map((row) => row.textContent ?? "");
    const exactIndex = rows.findIndex((text) => text.includes("照片") && !text.includes("我的照片") && !text.includes("备份"));
    const laterIndex = rows.findIndex((text) => text.includes("我的照片"));
    expect(exactIndex).toBeLessThan(laterIndex);
  });

  it("drops hidden selections when the search filter changes", async () => {
    render(<FileBrowser />);

    await waitFor(() => expect(screen.getByText("Alpha.txt")).toBeInTheDocument());

    fireEvent.click(screen.getByText("Alpha.txt"));
    expect(screen.getByText("导出 (1)")).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("搜索文件"), { target: { value: "beta" } });
    expect(screen.queryByText("导出 (1)")).not.toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("搜索文件"), { target: { value: "" } });

    expect(screen.queryByText("导出 (1)")).not.toBeInTheDocument();
    expect(screen.queryByText("删除 (1)")).not.toBeInTheDocument();
  });

  it("clears file search when browse context changes", async () => {
    render(<FileBrowser />);

    await waitFor(() => expect(screen.getByText("Alpha.txt")).toBeInTheDocument());

    const input = screen.getByPlaceholderText("搜索文件") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "beta" } });
    expect(input.value).toBe("beta");

    act(() => {
      useStore.setState({ currentPath: "/Documents" });
    });

    await waitFor(() => expect((screen.getByPlaceholderText("搜索文件") as HTMLInputElement).value).toBe(""));
  });
});

describe("FileBrowser marquee", () => {
  it("marquee selects entries intersecting the dragged rectangle", async () => {
    render(<FileBrowser />);
    await waitFor(() => expect(screen.getByText("Alpha.txt")).toBeInTheDocument());

    const entries = document.querySelectorAll<HTMLElement>("[data-file-entry]");
    entries.forEach((el, i) => {
      el.getBoundingClientRect = () => domRect(0, i * 30, 100, 20);
    });

    const container = screen.getByLabelText("文件浏览区域");
    fireEvent.mouseDown(container, { button: 0, clientX: 0, clientY: 0 });
    fireEvent.mouseMove(window, { clientX: 100, clientY: 55 });
    expect(screen.getByTestId("marquee-overlay")).toBeInTheDocument();
    fireEvent.mouseUp(window, { clientX: 100, clientY: 55 });

    // Rows 0 (0-20) and 1 (30-50) intersect; rows 2 (60-80) and 3 (90-110) do not.
    await waitFor(() => expect(screen.getByText("导出 (2)")).toBeInTheDocument());
  });
});
