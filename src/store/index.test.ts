import { describe, it, expect, beforeEach } from "vitest";
import { useStore } from "./index";
import { act, renderHook } from "@testing-library/react";

describe("useStore", () => {
  beforeEach(() => {
    useStore.setState({
      devices: [],
      selectedDeviceId: null,
      selectedApp: null,
      currentPath: "/",
      files: [],
      transfers: [],
      viewMode: "list",
    });
  });

  it("should set selected device", () => {
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.setSelectedDeviceId("device-1");
    });
    expect(result.current.selectedDeviceId).toBe("device-1");
  });

  it("should set view mode", () => {
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.setViewMode("grid");
    });
    expect(result.current.viewMode).toBe("grid");
  });

  it("should navigate to path", () => {
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.setCurrentPath("/Documents/images");
    });
    expect(result.current.currentPath).toBe("/Documents/images");
  });
});
