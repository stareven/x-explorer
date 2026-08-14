import { useStore } from "../../store";
import { Device } from "../../store";

const STATUS_LABEL: Record<Device["status"], string> = {
  connected: "已连接",
  unauthorized: "待信任/未授权",
  offline: "离线",
};

const STATUS_COLOR: Record<Device["status"], string> = {
  connected: "bg-green-400",
  unauthorized: "bg-yellow-400",
  offline: "bg-red-400",
};

export function DeviceList() {
  const devices = useStore((s) => s.devices);
  const selectedId = useStore((s) => s.selectedDeviceId);
  const setSelectedDeviceId = useStore((s) => s.setSelectedDeviceId);

  return (
    <div className="p-2">
      <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">设备</p>
      {devices.length === 0 && (
        <p className="text-xs text-gray-500 px-2">未检测到设备</p>
      )}
      {devices.map((device) => (
        <button
          key={device.id}
          onClick={() => setSelectedDeviceId(device.id)}
          title={STATUS_LABEL[device.status]}
          className={`w-full text-left px-3 py-2 rounded text-sm flex items-center gap-2 ${
            selectedId === device.id
              ? "bg-blue-600 text-white"
              : "text-gray-200 hover:bg-gray-700"
          }`}
        >
          <span>{device.platform === "ios" ? "📱" : "🤖"}</span>
          <span className="truncate">{device.name}</span>
          <span
            data-status={device.status}
            className={`ml-auto w-2 h-2 rounded-full ${STATUS_COLOR[device.status]}`}
          />
        </button>
      ))}
    </div>
  );
}
