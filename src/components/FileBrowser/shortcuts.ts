export type FileBrowserShortcutAction =
  | "back"
  | "forward"
  | "up"
  | "bookmark"
  | "upload"
  | "download"
  | "delete"
  | "select-all"
  | "goto";

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
    case "g":
      return "goto";
    default:
      return null;
  }
}

export interface FileBrowserShortcutInfo {
  action: FileBrowserShortcutAction;
  keys: string;
  description: string;
}

export const FILE_BROWSER_SHORTCUTS: FileBrowserShortcutInfo[] = [
  { action: "back", keys: "⌘ [", description: "后退" },
  { action: "forward", keys: "⌘ ]", description: "前进" },
  { action: "up", keys: "⌘ ↑", description: "上级目录" },
  { action: "bookmark", keys: "⌘ B", description: "收藏 / 取消收藏" },
  { action: "upload", keys: "⌘ U", description: "上传" },
  { action: "download", keys: "⌘ S", description: "下载" },
  { action: "delete", keys: "⌘ ⌫", description: "删除" },
  { action: "select-all", keys: "⌘ A", description: "全选" },
  { action: "goto", keys: "⌘ G", description: "跳转到目录" },
];

