import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook } from "@testing-library/react";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { listen } from "@tauri-apps/api/event";
import { useTransferListener, type TransferProgress } from "./useTauri";

// Capture the listener callback that `useTransferListener` registers with the
// Tauri event bus, so individual tests can simulate `transfer-progress`
// events without a real backend.
type ProgressEvent = { payload: TransferProgress };
type Handler = (event: ProgressEvent) => void;

function captureHandler() {
  let handler: Handler | null = null;
  vi.mocked(listen).mockImplementation(async (_event, h) => {
    handler = h as Handler;
    return () => {
      handler = null;
    };
  });
  return () => {
    if (!handler) throw new Error("useTransferListener did not register a listener");
    return handler;
  };
}

function makeProgress(overrides: Partial<TransferProgress> = {}): TransferProgress {
  return {
    task_id: "task-1",
    kind: "upload",
    src: "/local/a.txt",
    dst: "/Documents/a.txt",
    total_files: 2,
    completed_files: 0,
    status: "running",
    ...overrides,
  };
}

describe("useTransferListener", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("fires onTaskComplete when a task transitions to done", () => {
    const getHandler = captureHandler();
    const onTaskComplete = vi.fn();
    renderHook(() => useTransferListener(onTaskComplete));

    const handler = getHandler();
    handler({ payload: makeProgress({ status: "running", completed_files: 0 }) });
    handler({ payload: makeProgress({ status: "running", completed_files: 1 }) });
    handler({ payload: makeProgress({ status: "done", completed_files: 2 }) });

    expect(onTaskComplete).toHaveBeenCalledTimes(1);
    expect(onTaskComplete).toHaveBeenCalledWith(
      expect.objectContaining({ task_id: "task-1", status: "done", kind: "upload" })
    );
  });

  it("fires onTaskComplete when a task transitions to error", () => {
    const getHandler = captureHandler();
    const onTaskComplete = vi.fn();
    renderHook(() => useTransferListener(onTaskComplete));

    const handler = getHandler();
    handler({ payload: makeProgress({ status: "running" }) });
    handler({ payload: makeProgress({ status: "error", error: "boom" }) });

    expect(onTaskComplete).toHaveBeenCalledTimes(1);
    expect(onTaskComplete).toHaveBeenCalledWith(
      expect.objectContaining({ status: "error", error: "boom" })
    );
  });

  it("does not fire onTaskComplete for non-terminal status updates", () => {
    const getHandler = captureHandler();
    const onTaskComplete = vi.fn();
    renderHook(() => useTransferListener(onTaskComplete));

    const handler = getHandler();
    handler({ payload: makeProgress({ status: "pending" }) });
    handler({ payload: makeProgress({ status: "running", completed_files: 1 }) });
    handler({ payload: makeProgress({ status: "running", completed_files: 2 }) });

    expect(onTaskComplete).not.toHaveBeenCalled();
  });

  it("does not fire onTaskComplete a second time if a terminal status is re-emitted", () => {
    const getHandler = captureHandler();
    const onTaskComplete = vi.fn();
    renderHook(() => useTransferListener(onTaskComplete));

    const handler = getHandler();
    handler({ payload: makeProgress({ status: "running" }) });
    handler({ payload: makeProgress({ status: "done", completed_files: 2 }) });
    // Backend may re-emit the terminal state — we must not double-reload.
    handler({ payload: makeProgress({ status: "done", completed_files: 2 }) });

    expect(onTaskComplete).toHaveBeenCalledTimes(1);
  });

  it("treats terminal transitions per-task id, not globally", () => {
    const getHandler = captureHandler();
    const onTaskComplete = vi.fn();
    renderHook(() => useTransferListener(onTaskComplete));

    const handler = getHandler();
    handler({ payload: makeProgress({ task_id: "task-1", status: "running" }) });
    handler({ payload: makeProgress({ task_id: "task-1", status: "done", completed_files: 1 }) });
    handler({ payload: makeProgress({ task_id: "task-2", status: "running" }) });
    handler({ payload: makeProgress({ task_id: "task-2", status: "done", completed_files: 3 }) });

    expect(onTaskComplete).toHaveBeenCalledTimes(2);
    expect(onTaskComplete).toHaveBeenNthCalledWith(1, expect.objectContaining({ task_id: "task-1" }));
    expect(onTaskComplete).toHaveBeenNthCalledWith(2, expect.objectContaining({ task_id: "task-2" }));
  });
});
