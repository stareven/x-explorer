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
  files: FileEntry[];
  transfers: TransferTask[];
  viewMode: "list" | "grid";

  setDevices: (devices: Device[]) => void;
  setSelectedDeviceId: (id: string | null) => void;
  setSelectedApp: (app: AppInfo | null) => void;
  setBrowseTarget: (target: BrowseTarget | null) => void;
  setCurrentPath: (path: string) => void;
  setFiles: (files: FileEntry[]) => void;
  upsertTransfer: (task: TransferTask) => void;
  setViewMode: (mode: "list" | "grid") => void;
}

export const useStore = create<StoreState>((set) => ({
  devices: [],
  selectedDeviceId: null,
  selectedApp: null,
  browseTarget: null,
  currentPath: "/",
  files: [],
  transfers: [],
  viewMode: "list",

  setDevices: (devices) => set({ devices }),
  setSelectedDeviceId: (id) =>
    set({ selectedDeviceId: id, selectedApp: null, browseTarget: null, currentPath: "/", files: [] }),
  setSelectedApp: (app) =>
    set({
      selectedApp: app,
      browseTarget: app ? { kind: "app", app } : null,
      currentPath: "/",
      files: [],
    }),
  setBrowseTarget: (target) =>
    set({
      browseTarget: target,
      selectedApp: target?.kind === "app" ? target.app : null,
      currentPath: "/",
      files: [],
    }),
  setCurrentPath: (path) => set({ currentPath: path }),
  setFiles: (files) => set({ files }),
  upsertTransfer: (task) =>
    set((s) => ({
      transfers: s.transfers.find((t) => t.id === task.id)
        ? s.transfers.map((t) => (t.id === task.id ? task : t))
        : [...s.transfers, task],
    })),
  setViewMode: (mode) => set({ viewMode: mode }),
}));
