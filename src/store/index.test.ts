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
      navHistory: ["/"],
      navIndex: 0,
      files: [],
      transfers: [],
      viewMode: "list",
      browseTarget: null,
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

  it("should go back and forward through navigation history", () => {
    const { result } = renderHook(() => useStore());

    act(() => {
      result.current.navigate("/Documents");
      result.current.navigate("/Documents/images");
      result.current.goBack();
    });

    expect(result.current.currentPath).toBe("/Documents");
    expect(result.current.navIndex).toBe(1);

    act(() => {
      result.current.goForward();
    });

    expect(result.current.currentPath).toBe("/Documents/images");
    expect(result.current.navIndex).toBe(2);

    act(() => {
      result.current.goForward();
    });

    expect(result.current.currentPath).toBe("/Documents/images");
    expect(result.current.navIndex).toBe(2);
  });

  it("should insert a new transfer and update it in place on repeated upsert with same id", () => {
    const { result } = renderHook(() => useStore());
    act(() => {
      result.current.upsertTransfer({
        id: "task-1",
        kind: "download",
        src: "/remote/a",
        dst: "/local/a",
        total_files: 1,
        completed_files: 0,
        status: "pending",
      });
    });
    expect(result.current.transfers).toHaveLength(1);
    expect(result.current.transfers[0]).toMatchObject({ id: "task-1", status: "pending" });

    act(() => {
      result.current.upsertTransfer({
        id: "task-1",
        kind: "download",
        src: "/remote/a",
        dst: "/local/a",
        total_files: 1,
        completed_files: 1,
        status: "running",
      });
    });
    expect(result.current.transfers).toHaveLength(1);
    expect(result.current.transfers[0]).toMatchObject({
      id: "task-1",
      status: "running",
      completed_files: 1,
    });
  });

  it("should set selectedApp and its side effects via setSelectedApp", () => {
    const { result } = renderHook(() => useStore());
    const app = { bundle_id: "com.example.app", name: "Example" };
    act(() => {
      result.current.setCurrentPath("/some/path");
      result.current.setFiles([
        { name: "f.txt", path: "/some/path/f.txt", is_dir: false, size: 1 },
      ]);
    });
    act(() => {
      result.current.setSelectedApp(app);
    });
    expect(result.current.selectedApp).toEqual(app);
    expect(result.current.browseTarget).toEqual({ kind: "app", app });
    expect(result.current.currentPath).toBe("/");
    expect(result.current.files).toEqual([]);
  });

  it("should set browseTarget and reset selectedApp for non-app targets via setBrowseTarget", () => {
    const { result } = renderHook(() => useStore());
    const app = { bundle_id: "com.example.app", name: "Example" };
    act(() => {
      result.current.setSelectedApp(app);
    });
    expect(result.current.selectedApp).toEqual(app);

    act(() => {
      result.current.setBrowseTarget({ kind: "external-storage" });
    });
    expect(result.current.browseTarget).toEqual({ kind: "external-storage" });
    expect(result.current.selectedApp).toBeNull();
    expect(result.current.currentPath).toBe("/sdcard");
    expect(result.current.files).toEqual([]);
  });
});
