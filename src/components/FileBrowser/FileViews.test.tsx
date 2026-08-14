import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { FileList } from "./FileList";
import { FileEntry } from "../../store";

const mockFiles: FileEntry[] = [
  { name: "Documents", path: "/Documents", is_dir: true, size: 0 },
  { name: "config.json", path: "/config.json", is_dir: false, size: 1024 },
];

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
      />
    );
    expect(screen.getByText("1.0 KB")).toBeInTheDocument();
  });
});
