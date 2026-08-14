import { useEffect, useState } from "react";
import { DevicePanel } from "./components/DevicePanel";
import { FileBrowser } from "./components/FileBrowser";
import { TransferPanel } from "./components/TransferPanel";
import { useDeviceListener } from "./hooks/useTauri";
import { tauriApi } from "./hooks/useTauri";
import { useStore } from "./store";

export default function App() {
  useDeviceListener();

  const setDevices = useStore((s) => s.setDevices);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    const load = async () => {
      const [ios, android] = await Promise.allSettled([
        tauriApi.listIosDevices(),
        tauriApi.listAndroidDevices(),
      ]);
      const all = [
        ...(ios.status === "fulfilled" ? ios.value : []),
        ...(android.status === "fulfilled" ? android.value : []),
      ];
      const errors = [
        ios.status === "rejected" ? `iOS 设备检测失败: ${ios.reason}` : null,
        android.status === "rejected" ? `Android 设备检测失败: ${android.reason}` : null,
      ].filter((e): e is string => e !== null);
      setLoadError(errors.length > 0 ? errors.join("; ") : null);
      setDevices(all);
    };
    load();
  }, [setDevices]);

  return (
    <div className="flex flex-col h-screen bg-gray-900 text-white overflow-hidden">
      {loadError && (
        <div className="px-3 py-2 bg-red-900 text-red-200 text-xs flex items-center justify-between">
          <span>{loadError}</span>
          <button onClick={() => setLoadError(null)} className="text-red-300 hover:text-white">
            ✕
          </button>
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <DevicePanel />
        <FileBrowser />
      </div>
      <TransferPanel />
    </div>
  );
}
