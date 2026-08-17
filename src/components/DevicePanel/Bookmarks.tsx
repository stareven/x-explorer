import { DirBookmark, useStore } from "../../store";

export function Bookmarks() {
  const bookmarks = useStore((s) => s.bookmarks);
  const removeBookmark = useStore((s) => s.removeBookmark);
  const devices = useStore((s) => s.devices);
  const selectedDeviceId = useStore((s) => s.selectedDeviceId);
  const openBookmark = useStore((s) => s.openBookmark);

  if (bookmarks.length === 0) return null;

  // Bookmarks are device-agnostic (platform only): jump on the currently
  // selected device when it matches, otherwise the first connected device of
  // that platform. Ignore the click when no compatible device is connected.
  function jump(b: DirBookmark) {
    const current = devices.find((d) => d.id === selectedDeviceId);
    const target =
      current && current.platform === b.platform && current.status === "connected"
        ? current
        : devices.find((d) => d.platform === b.platform && d.status === "connected");
    if (!target) return;
    openBookmark(
      target.id,
      b.app ? { kind: "app", app: b.app } : { kind: "external-storage" },
      b.path,
    );
  }

  return (
    <div className="p-2 border-t border-gray-700">
      <p className="text-xs font-semibold text-gray-400 uppercase px-2 mb-1">快速访问</p>
      {bookmarks.map((b) => {
        const label = b.app ? b.app.name : "外部存储";
        const dir = b.path === "/" ? "/" : b.path.split("/").filter(Boolean).pop();
        const tooltip = b.app ? `${b.app.bundle_id}${b.path}` : b.path;
        return (
          <div key={`${b.platform}:${b.app?.bundle_id ?? ""}:${b.path}`} className="group flex items-center">
            <button
              onClick={() => jump(b)}
              className="flex-1 min-w-0 text-left px-3 py-1.5 rounded text-xs hover:bg-gray-700 text-gray-300"
              title={tooltip}
            >
              <div className="flex min-w-0 flex-col gap-0.5">
                <span className="font-medium truncate">{label}</span>
                {b.app ? (
                  <span className="truncate text-gray-500">
                    {b.app.bundle_id}
                    {dir && dir !== "/" ? ` / ${dir}` : ""}
                  </span>
                ) : (
                  dir && dir !== "/" && <span className="truncate text-gray-500">{dir}</span>
                )}
              </div>
            </button>
            <button
              onClick={() => removeBookmark(b)}
              className="px-2 py-1 text-xs text-gray-500 hover:text-red-400 opacity-0 group-hover:opacity-100"
              aria-label="删除书签"
              title="删除书签"
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
