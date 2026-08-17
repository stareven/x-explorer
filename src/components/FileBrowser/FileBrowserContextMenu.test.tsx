import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FileBrowserContextMenu } from "./FileBrowserContextMenu";

describe("FileBrowserContextMenu", () => {
  it("renders actions at the requested coordinates", () => {
    render(
      <FileBrowserContextMenu
        x={24}
        y={32}
        actions={[{ label: "导入", onAction: vi.fn() }]}
        onClose={vi.fn()}
      />
    );

    const menu = screen.getByRole("menu");
    expect(menu).toHaveStyle({ left: "24px", top: "32px" });
    expect(screen.getByRole("menuitem", { name: "导入" })).toBeInTheDocument();
  });

  it("runs the action and closes when a menu item is clicked", () => {
    const onAction = vi.fn();
    const onClose = vi.fn();

    render(
      <FileBrowserContextMenu
        x={24}
        y={32}
        actions={[{ label: "导出", onAction }]}
        onClose={onClose}
      />
    );

    fireEvent.click(screen.getByRole("menuitem", { name: "导出" }));

    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
