import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ShortcutsOverlay } from "./ShortcutsOverlay";
import { FILE_BROWSER_SHORTCUTS } from "./FileBrowser/shortcuts";

describe("ShortcutsOverlay", () => {
  it("renders nothing when not visible", () => {
    render(<ShortcutsOverlay visible={false} />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders all shortcuts when visible", () => {
    render(<ShortcutsOverlay visible={true} />);

    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeTruthy();

    for (const shortcut of FILE_BROWSER_SHORTCUTS) {
      expect(screen.getByText(shortcut.keys)).toBeTruthy();
      expect(screen.getByText(shortcut.description)).toBeTruthy();
    }
  });

  it("does not block pointer events so clicks pass through", () => {
    const { container } = render(<ShortcutsOverlay visible={true} />);

    const overlay = container.firstElementChild;
    expect(overlay).toBeTruthy();
    expect(overlay!.className).toContain("pointer-events-none");
  });
});
