export type FileBrowserShortcutAction =
  | "back"
  | "forward"
  | "up"
  | "bookmark"
  | "upload"
  | "download"
  | "delete"
  | "select-all";

type ShortcutEvent = Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "target">;

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;

  switch (target.tagName) {
    case "INPUT":
      return !(target as HTMLInputElement).disabled && !(target as HTMLInputElement).readOnly;
    case "TEXTAREA":
      return !(target as HTMLTextAreaElement).disabled && !(target as HTMLTextAreaElement).readOnly;
    case "SELECT":
      return !(target as HTMLSelectElement).disabled;
    default:
      return (
        target.isContentEditable ||
        target.contentEditable === "true" ||
        target.getAttribute("contenteditable") === "true" ||
        Boolean(target.closest('[contenteditable="true"]'))
      );
  }
}

export function getFileBrowserShortcutAction(event: ShortcutEvent): FileBrowserShortcutAction | null {
  if (!event.metaKey || event.ctrlKey || event.altKey) return null;
  if (isEditableTarget(event.target)) return null;

  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  switch (key) {
    case "[":
      return "back";
    case "]":
      return "forward";
    case "ArrowUp":
      return "up";
    case "b":
      return "bookmark";
    case "u":
      return "upload";
    case "s":
      return "download";
    case "Backspace":
      return "delete";
    case "a":
      return "select-all";
    default:
      return null;
  }
}



