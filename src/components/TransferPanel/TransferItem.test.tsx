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
});
