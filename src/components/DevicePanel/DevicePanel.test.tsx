import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DeviceList } from "./DeviceList";
import { Device, useStore } from "../../store";

const mockDevices: Device[] = [
  { id: "iphone-1", name: "iPhone 15", platform: "ios", status: "connected" },
  { id: "pixel-1", name: "Pixel 7", platform: "android", status: "connected" },
  { id: "old-phone", name: "Old Phone", platform: "android", status: "unauthorized" },
];

beforeEach(() => {
  useStore.setState({ devices: mockDevices, selectedDeviceId: null });
});

describe("DeviceList", () => {
  it("renders device names and ids", () => {
    render(<DeviceList />);
    expect(screen.getByText("iPhone 15")).toBeInTheDocument();
    expect(screen.getByText("iphone-1")).toBeInTheDocument();
    expect(screen.getByText("Pixel 7")).toBeInTheDocument();
    expect(screen.getByText("pixel-1")).toBeInTheDocument();
  });

  it("selects device on click", () => {
    render(<DeviceList />);
    fireEvent.click(screen.getByText("iPhone 15"));
    expect(useStore.getState().selectedDeviceId).toBe("iphone-1");
  });

  it("shows a distinct status badge for unauthorized devices", () => {
    render(<DeviceList />);
    const row = screen.getByText("Old Phone").closest("button")!;
    expect(row.querySelector("[data-status='unauthorized']")).toBeInTheDocument();
  });
});
