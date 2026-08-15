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
}));
