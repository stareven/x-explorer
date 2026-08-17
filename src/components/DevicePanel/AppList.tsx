import { useEffect, useState } from "react";
import { AppInfo, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

export function AppList() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const devices = useStore((s) => s.devices);
  const browseTarget = useStore((s) => s.browseTarget);
  const setBrowseTarget = useStore((s) => s.setBrowseTarget);
  const favoriteAppIds = useStore((s) => s.favoriteAppIds);
  const toggleFavoriteApp = useStore((s) => s.toggleFavoriteApp);
  const [apps, setApps] = useState<AppInfo[]>([]);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const isUsable = device && device.status === "connected";

  useEffect(() => {
    if (!isUsable || !device) {
      setApps([]);
      return;
    }
    if (device.platform !== "ios") {
      setApps([]);
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const list = await tauriApi.listIosApps(device.id);
        if (!cancelled) {
          setApps(list.sort((a, b) => a.name.localeCompare(b.name)));
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

  const favorites = apps.filter((a) => favoriteAppIds.includes(a.bundle_id));
  const others = apps.filter((a) => !favoriteAppIds.includes(a.bundle_id));

  function renderApp(app: AppInfo, isFavorite: boolean) {
    const active = browseTarget?.kind === "app" && browseTarget.app.bundle_id === app.bundle_id;
    return (
      <div key={app.bundle_id} className="flex items-center">
        <button
          onClick={() => setBrowseTarget({ kind: "app", app })}
          className={`flex-1 min-w-0 text-left px-3 py-1.5 rounded text-xs ${
            active ? "bg-blue-600 text-white" : "text-gray-300 hover:bg-gray-700"
          }`}
        >
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="truncate">{app.name}</span>
            <span className={`truncate text-[11px] ${active ? "text-blue-100" : "text-gray-500"}`}>
              {app.bundle_id}
            </span>
          </div>
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

  if (device.platform === "android") {
    return (
      <div className="p-2 border-t border-gray-700">
        <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">外部存储</p>
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
      </div>
    );
  }

  return (
    <div className="p-2 border-t border-gray-700">
      <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">应用</p>
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
