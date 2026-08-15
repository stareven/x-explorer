import { useEffect, useState } from "react";
import { AppInfo, useStore } from "../../store";
import { tauriApi } from "../../hooks/useTauri";

export function AppList() {
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const devices = useStore((s) => s.devices);
  const browseTarget = useStore((s) => s.browseTarget);
  const setBrowseTarget = useStore((s) => s.setBrowseTarget);
  const [apps, setApps] = useState<AppInfo[]>([]);

  const device = devices.find((d) => d.id === selectedDeviceId);
  const isUsable = device && device.status === "connected";

  useEffect(() => {
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

  return (
    <div className="p-2 border-t border-gray-700">
      <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">应用</p>
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
      {apps.map((app) => (
        <button
          key={app.bundle_id}
          onClick={() => setBrowseTarget({ kind: "app", app })}
          className={`w-full text-left px-3 py-1.5 rounded text-xs truncate ${
            browseTarget?.kind === "app" && browseTarget.app.bundle_id === app.bundle_id
              ? "bg-blue-600 text-white"
              : "text-gray-300 hover:bg-gray-700"
          }`}
        >
          {app.name}
        </button>
      ))}
    </div>
  );
}
