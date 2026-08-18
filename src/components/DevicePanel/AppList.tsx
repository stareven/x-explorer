import { useEffect, useRef, useState } from "react";
import { AppInfo, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";
import { buildSearchIndex, compareSearchMatches, rankSearchIndex, SearchIndex } from "../../utils/search";
import { FOCUS_APP_SEARCH_EVENT } from "../FileBrowser";

interface IndexedAppInfo extends AppInfo {
  search_index: SearchIndex;
}

export function AppList() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const devices = useStore((s) => s.devices);
  const browseTarget = useStore((s) => s.browseTarget);
  const setBrowseTarget = useStore((s) => s.setBrowseTarget);
  const favoriteAppIds = useStore((s) => s.favoriteAppIds);
  const toggleFavoriteApp = useStore((s) => s.toggleFavoriteApp);
  const [apps, setApps] = useState<IndexedAppInfo[]>([]);
  const [appSearch, setAppSearch] = useState("");
  const appSearchInputRef = useRef<HTMLInputElement>(null);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const isUsable = device && device.status === "connected";

  useEffect(() => {
    const handler = () => {
      appSearchInputRef.current?.focus();
      appSearchInputRef.current?.select();
    };
    window.addEventListener(FOCUS_APP_SEARCH_EVENT, handler);
    return () => window.removeEventListener(FOCUS_APP_SEARCH_EVENT, handler);
  }, []);

  useEffect(() => {
    setAppSearch("");
    if (!isUsable || !device) {
      setApps([]);
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const list =
          device.platform === "ios"
            ? await tauriApi.listIosApps(device.id)
            : await tauriApi.listAndroidApps(device.id);
        if (!cancelled) {
          setApps(list.map((app) => ({ ...app, search_index: buildSearchIndex(app.name, app.bundle_id) })).sort((a, b) => a.name.localeCompare(b.name)));
        }
      } catch (e) {
        console.error("Failed to load apps:", e);
        if (!cancelled) {
          setApps([]);
        }
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, [device, isUsable]);

  if (!device) return null;

  if (!isUsable) {
    return (
      <div className="p-2 border-t border-gray-700">
        <p className="text-xs text-yellow-400 px-2">
          设备待信任或未授权，请在设备上确认后重试
        </p>
      </div>
    );
  }

  const normalizedSearch = appSearch.trim().toLowerCase();
  const filteredApps = normalizedSearch
    ? apps
        .map((app) => ({ app, match: rankSearchIndex(app.search_index, appSearch) }))
        .filter((entry): entry is { app: IndexedAppInfo; match: NonNullable<typeof entry.match> } => entry.match != null)
        .sort((a, b) => compareSearchMatches(a.match, b.match) || a.app.name.localeCompare(b.app.name))
        .map((entry) => entry.app)
    : apps;
  const favorites = filteredApps.filter((a) => favoriteAppIds.includes(a.bundle_id));
  const others = filteredApps.filter((a) => !favoriteAppIds.includes(a.bundle_id));

  function renderApp(app: AppInfo, isFavorite: boolean) {
    const active = browseTarget?.kind === "app" && browseTarget.app.bundle_id === app.bundle_id;
    return (
      <div key={app.bundle_id} className="flex items-center">
        <button
          onClick={() => setBrowseTarget({ kind: "app", app })}
          className={`flex-1 min-w-0 text-left px-3 py-1.5 rounded text-xs truncate ${
            active ? "bg-blue-600 text-white" : "text-gray-300 hover:bg-gray-700"
          }`}
        >
          {app.name}
        </button>
        <button
          onClick={() => toggleFavoriteApp(app.bundle_id)}
          className={`px-1.5 py-1 text-xs ${isFavorite ? "text-yellow-400" : "text-gray-600 hover:text-gray-400"}`}
          aria-label={isFavorite ? "取消收藏" : "收藏"}
          title={isFavorite ? "取消收藏" : "收藏"}
        >
          {isFavorite ? "★" : "☆"}
        </button>
      </div>
    );
  }

  return (
    <div className="p-2 border-t border-gray-700">
      <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">应用</p>
      <input
        ref={appSearchInputRef}
        value={appSearch}
        onChange={(e) => setAppSearch(e.target.value)}
        placeholder="搜索应用"
        className="w-full mb-2 px-3 py-1.5 rounded bg-gray-800 border border-gray-700 text-sm text-gray-100 placeholder:text-gray-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
      />
      {device.platform === "android" && (
        <button
          onClick={() => setBrowseTarget({ kind: "external-storage" })}
          className={`w-full text-left px-3 py-1.5 rounded text-xs truncate flex items-center gap-2 ${
            browseTarget?.kind === "external-storage"
              ? "bg-blue-600 text-white"
              : "text-gray-300 hover:bg-gray-700"
          }`}
        >
          <span>💾</span>
          <span>外部存储</span>
        </button>
      )}
      {favorites.length > 0 && (
        <>
          <p className="text-xs text-gray-500 px-2 mt-2 mb-1">常用</p>
          {favorites.map((app) => renderApp(app, true))}
          <p className="text-xs text-gray-500 px-2 mt-2 mb-1">全部</p>
        </>
      )}
      {others.map((app) => renderApp(app, false))}
    </div>
  );
}
