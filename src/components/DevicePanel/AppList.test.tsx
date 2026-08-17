import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { AppList } from "./AppList";
import { AppInfo, Device, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

vi.mock("../../hooks/useTauri", () => ({
  tauriApi: {
    listIosApps: vi.fn(),
    listAndroidApps: vi.fn(),
  },
}));

const iosApps: AppInfo[] = [
  { name: "备忘录", bundle_id: "com.apple.mobilenotes" },
  { name: "照片", bundle_id: "com.apple.mobileslideshow" },
  { name: "Files", bundle_id: "com.apple.DocumentsApp" },
];

const androidApps: AppInfo[] = [
  { name: "Settings", bundle_id: "com.android.settings" },
  { name: "相机", bundle_id: "com.google.camera" },
];

const devices: Device[] = [
  { id: "iphone-1", name: "iPhone", platform: "ios", status: "connected" },
  { id: "pixel-1", name: "Pixel", platform: "android", status: "connected" },
];

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(tauriApi.listIosApps).mockResolvedValue(iosApps);
  vi.mocked(tauriApi.listAndroidApps).mockResolvedValue(androidApps);
  useStore.setState({
    devices,
    selectedDeviceId: "iphone-1",
    browseTarget: null,
    favoriteAppIds: [],
  });
});

describe("AppList search", () => {
  it("filters apps by visible name while typing", async () => {
    render(<AppList />);

    await waitFor(() => expect(screen.getByText("备忘录")).toBeInTheDocument());

    fireEvent.change(screen.getByPlaceholderText("搜索应用"), {
      target: { value: "照片" },
    });

    expect(screen.getByText("照片")).toBeInTheDocument();
    expect(screen.queryByText("备忘录")).not.toBeInTheDocument();
    expect(screen.queryByText("Files")).not.toBeInTheDocument();
  });

  it("filters apps by bundle id while typing", async () => {
    render(<AppList />);

    await waitFor(() => expect(screen.getByText("Files")).toBeInTheDocument());

    fireEvent.change(screen.getByPlaceholderText("搜索应用"), {
      target: { value: "documentsapp" },
    });

    expect(screen.getByText("Files")).toBeInTheDocument();
    expect(screen.queryByText("备忘录")).not.toBeInTheDocument();
    expect(screen.queryByText("照片")).not.toBeInTheDocument();
  });

  it("orders app search results by match quality", async () => {
    vi.mocked(tauriApi.listIosApps).mockResolvedValue([
      { name: "我的照片", bundle_id: "com.example.later" },
      { name: "照片", bundle_id: "com.example.exact" },
      { name: "照片备份", bundle_id: "com.example.backup" },
    ]);

    render(<AppList />);

    await waitFor(() => expect(screen.getByText("我的照片")).toBeInTheDocument());
    fireEvent.change(screen.getByPlaceholderText("搜索应用"), { target: { value: "照片" } });

    const buttons = screen.getAllByRole("button").map((button) => button.textContent ?? "");
    expect(buttons.findIndex((text) => text.includes("照片") && !text.includes("我的照片"))).toBeLessThan(
      buttons.findIndex((text) => text.includes("我的照片")),
    );
  });

  it("clears app search when selected device changes", async () => {
    render(<AppList />);

    await waitFor(() => expect(screen.getByText("备忘录")).toBeInTheDocument());

    fireEvent.change(screen.getByPlaceholderText("搜索应用"), {
      target: { value: "照片" },
    });
    expect(screen.queryByText("备忘录")).not.toBeInTheDocument();

    useStore.setState({ selectedDeviceId: "pixel-1" });

    await waitFor(() => expect(screen.getByText("Settings")).toBeInTheDocument());
    expect(screen.getByPlaceholderText("搜索应用")).toHaveValue("");
    expect(screen.getByText("相机")).toBeInTheDocument();
  });
});
