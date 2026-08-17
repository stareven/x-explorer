import { create } from "zustand";

export interface Device {
  id: string;
  name: string;
  platform: "ios" | "android";
  status: "connected" | "unauthorized" | "offline";
}

export interface AppInfo {
  bundle_id: string;
  name: string;
}

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified?: number;
}

export interface TransferTask {
  id: string;
  kind: "upload" | "download";
  src: string;
  dst: string;
  total_bytes: number;
  transferred_bytes: number;
  status: "pending" | "running" | "done" | "error" | "cancelled";
  error?: string;
}

// Android browsing target: either a specific app's data directory (requires
// run-as bridging, needs `package`) or the fixed "external storage" entry
// point (/sdcard, no package needed). iOS always browses via selectedApp.
export type BrowseTarget =
  | { kind: "app"; app: AppInfo }
  | { kind: "external-storage" };

// Saved directory shortcut. Not tied to a specific device — only the platform
// matters, so a bookmark survives switching between devices of the same kind.
export interface DirBookmark {
  platform: "ios" | "android";
  app: AppInfo | null; // null = Android external storage
  path: string;
}

const FAV_KEY = "favoriteAppIds";
const BM_KEY = "dirBookmarks";

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

interface StoreState {
  devices: Device[];
  selectedDeviceId: string | null;
  selectedApp: AppInfo | null;
  browseTarget: BrowseTarget | null;
  currentPath: string;
  navHistory: string[];
  navIndex: number;
  files: FileEntry[];
  transfers: TransferTask[];
  viewMode: "list" | "grid";
  favoriteAppIds: string[];
  bookmarks: DirBookmark[];

  setDevices: (devices: Device[]) => void;
  setSelectedDeviceId: (id: string | null) => void;
  setSelectedApp: (app: AppInfo | null) => void;
  setBrowseTarget: (target: BrowseTarget | null) => void;
  setCurrentPath: (path: string) => void;
  navigate: (path: string) => void;
  goBack: () => void;
  setFiles: (files: FileEntry[]) => void;
  patchFileInfo: (path: string, info: { is_dir: boolean; size: number; modified?: number }) => void;
  upsertTransfer: (task: TransferTask) => void;
  setViewMode: (mode: "list" | "grid") => void;
  toggleFavoriteApp: (bundleId: string) => void;
  addBookmark: (bookmark: DirBookmark) => void;
  removeBookmark: (bookmark: DirBookmark) => void;
  openBookmark: (deviceId: string, target: BrowseTarget, path: string) => void;
}

/// Parent path of a browse path ("/a/b" -> "/a", "/a" -> "/", "/" -> "/").
export function parentPath(p: string): string {
  if (p === "/") return "/";
  const idx = p.lastIndexOf("/");
  return idx <= 0 ? "/" : p.slice(0, idx);
}

export const useStore = create<StoreState>((set) => ({
  devices: [],
  selectedDeviceId: null,
  selectedApp: null,
  browseTarget: null,
  currentPath: "/",
  navHistory: ["/"],
  navIndex: 0,
  files: [],
  transfers: [],
  viewMode: "list",
  favoriteAppIds: loadJson<string[]>(FAV_KEY, []),
  bookmarks: loadJson<DirBookmark[]>(BM_KEY, []),

  setDevices: (devices) => set({ devices }),
  setSelectedDeviceId: (id) =>
    set({ selectedDeviceId: id, selectedApp: null, browseTarget: null, currentPath: "/", navHistory: ["/"], navIndex: 0, files: [] }),
  setSelectedApp: (app) =>
    set({
      selectedApp: app,
      browseTarget: app ? { kind: "app", app } : null,
      currentPath: "/",
      navHistory: ["/"],
      navIndex: 0,
      files: [],
    }),
  setBrowseTarget: (target) =>
    set({
      browseTarget: target,
      selectedApp: target?.kind === "app" ? target.app : null,
      currentPath: "/",
      navHistory: ["/"],
      navIndex: 0,
      files: [],
    }),
  setCurrentPath: (path) => set({ currentPath: path }),
  // Navigation with history: truncates any forward entries (like a browser)
  // and records the new path so goBack can walk the stack.
  navigate: (path) =>
    set((s) => {
      const history = [...s.navHistory.slice(0, s.navIndex + 1), path];
      return { currentPath: path, navHistory: history, navIndex: history.length - 1 };
    }),
  goBack: () =>
    set((s) => {
      if (s.navIndex === 0) return {};
      return { navIndex: s.navIndex - 1, currentPath: s.navHistory[s.navIndex - 1] };
    }),
  setFiles: (files) =>
    set({ files: [...files].sort((a, b) => a.name.localeCompare(b.name)) }),
  patchFileInfo: (path, info) =>
    set((s) => ({
      files: s.files.map((f) => (f.path === path ? { ...f, ...info } : f)),
    })),
  upsertTransfer: (task) =>
    set((s) => ({
      transfers: s.transfers.find((t) => t.id === task.id)
        ? s.transfers.map((t) => (t.id === task.id ? task : t))
        : [...s.transfers, task],
    })),
  setViewMode: (mode) => set({ viewMode: mode }),
  toggleFavoriteApp: (bundleId) =>
    set((s) => {
      const favoriteAppIds = s.favoriteAppIds.includes(bundleId)
        ? s.favoriteAppIds.filter((id) => id !== bundleId)
        : [...s.favoriteAppIds, bundleId];
      localStorage.setItem(FAV_KEY, JSON.stringify(favoriteAppIds));
      return { favoriteAppIds };
    }),
  addBookmark: (bookmark) =>
    set((s) => {
      if (s.bookmarks.some((b) => sameBookmark(b, bookmark))) return {};
      const bookmarks = [...s.bookmarks, bookmark];
      localStorage.setItem(BM_KEY, JSON.stringify(bookmarks));
      return { bookmarks };
    }),
  removeBookmark: (bookmark) =>
    set((s) => {
      const bookmarks = s.bookmarks.filter((b) => !sameBookmark(b, bookmark));
      localStorage.setItem(BM_KEY, JSON.stringify(bookmarks));
      return { bookmarks };
    }),
  openBookmark: (deviceId, target, path) =>
    set({
      selectedDeviceId: deviceId,
      selectedApp: target.kind === "app" ? target.app : null,
      browseTarget: target,
      currentPath: path,
      navHistory: [path],
      navIndex: 0,
      files: [],
    }),
}));

function sameBookmark(a: DirBookmark, b: DirBookmark) {
  return (
    a.platform === b.platform &&
    (a.app?.bundle_id ?? "") === (b.app?.bundle_id ?? "") &&
    a.path === b.path
  );
}
