import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TransferItem } from "./TransferItem";
import { TransferTask } from "../../store";

vi.mock("../../hooks/useTauri", () => ({
  tauriApi: {
    cancelTransfer: vi.fn(),
  },
}));

describe("TransferItem", () => {
  it("calculates progress from completed file count", () => {
    const task: TransferTask = {
      id: "task-1",
      kind: "download",
      src: "/remote",
      dst: "/local",
      total_files: 4,
      completed_files: 3,
      status: "running",
    };

    const { container } = render(<TransferItem task={task} />);

    expect(container.querySelector(".h-full")).toHaveStyle({ width: "75%" });
    expect(screen.getByText("3/4")).toBeInTheDocument();
  });

  it("renders indeterminate progress when running with no completed files", () => {
    const task: TransferTask = {
      id: "task-indeterminate",
      kind: "delete",
      src: "/large-folder",
      dst: "/large-folder",
      total_files: 1,
      completed_files: 0,
      status: "running",
    };
    const { getByText, container } = render(<TransferItem task={task} />);
    // Shows "处理中…" instead of "0/1"
    expect(getByText("处理中…")).toBeTruthy();
    // Progress bar uses animate-pulse class
    const bar = container.querySelector(".h-full.rounded");
    expect(bar?.className).toContain("animate-pulse");
  });

  it("shows normal progress when completed_files > 0", () => {
    const task: TransferTask = {
      id: "task-progress",
      kind: "upload",
      src: "/local/files",
      dst: "/device/files",
      total_files: 5,
      completed_files: 3,
      status: "running",
    };
    const { getByText } = render(<TransferItem task={task} />);
    expect(getByText("3/5")).toBeTruthy();
  });
});
