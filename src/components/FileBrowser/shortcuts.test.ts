import { describe, it, expect } from "vitest";
import { getFileBrowserShortcutAction, isEditableTarget } from "./shortcuts";

describe("file browser shortcuts", () => {
  it("matches browser navigation and file actions", () => {
    expect(getFileBrowserShortcutAction({ key: "[", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("back");
    expect(getFileBrowserShortcutAction({ key: "]", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("forward");
    expect(getFileBrowserShortcutAction({ key: "ArrowUp", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("up");
    expect(getFileBrowserShortcutAction({ key: "b", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("bookmark");
    expect(getFileBrowserShortcutAction({ key: "u", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("upload");
    expect(getFileBrowserShortcutAction({ key: "u", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true, target: null })).toBe("upload-dir");
    expect(getFileBrowserShortcutAction({ key: "s", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("download");
    expect(getFileBrowserShortcutAction({ key: "Backspace", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("delete");
    expect(getFileBrowserShortcutAction({ key: "a", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("select-all");
    expect(getFileBrowserShortcutAction({ key: "g", metaKey: true, ctrlKey: false, altKey: false, target: null })).toBe("goto");
  });

  it("ignores shortcuts when focus is in an editable target", () => {
    const input = document.createElement("input");
    const textArea = document.createElement("textarea");
    const contentEditable = document.createElement("div");
    contentEditable.contentEditable = "true";

    expect(isEditableTarget(input)).toBe(true);
    expect(isEditableTarget(textArea)).toBe(true);
    expect(isEditableTarget(contentEditable)).toBe(true);
    expect(getFileBrowserShortcutAction({ key: "a", metaKey: true, ctrlKey: false, altKey: false, target: input })).toBe(null);
  });
});
